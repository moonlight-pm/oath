//! The daily security audit's scan (R48): every package this tree ships,
//! matched by `pkgs/audit.toml` against cve.org's CVE List V5 (a local
//! mirror of `CVEProject/cvelistV5`), OSV (ecosystem packages, and the
//! commit of an upstream's release tag), NVD (by CPE) and CISA's Known
//! Exploited Vulnerabilities, with FIRST's EPSS beside each. It writes
//! `build/audit/<day>.json` and `build/audit/latest.json` for the
//! `oath-audit` skill to triage and changes nothing it scans. A source
//! that cannot be reached is recorded as such, and the rest still run.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CVELIST: &str = "https://github.com/CVEProject/cvelistV5";
const OSV: &str = "https://api.osv.dev/v1";
const NVD: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";
const KEV: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
const EPSS: &str = "https://api.first.org/data/v1/epss";
/// Purl types OSV knows (an upstream's `pkg:generic` and `pkg:github` it
/// does not; those go by CPE, product and commit).
const ECOSYSTEMS: &[&str] = &["cargo", "npm", "pypi", "golang", "gem", "maven", "nuget", "hex"];

pub struct Opts {
    /// Only this package (`curl` or `pkg:curl`).
    pub name: Option<String>,
    /// Use the CVE List mirror as it is, and skip NVD (no network but OSV,
    /// KEV and EPSS, which are quick).
    pub offline: bool,
}

/// One package's entry in `pkgs/audit.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub cpe: Option<String>,
    #[serde(default)]
    pub names: Vec<String>,
    pub repo: Option<String>,
    pub tag: Option<String>,
    pub osv: Option<String>,
    pub version_after: Option<String>,
    pub skip: Option<String>,
    #[serde(default)]
    pub not_affected: BTreeMap<String, String>,
}

/// A package as the audit matches it.
#[derive(Clone, Debug, Default)]
struct Target {
    name: String,
    /// The upstream's version (`version_after` applied).
    version: String,
    entry: Entry,
    /// Ecosystem purls in its pack's `components.json`.
    purls: Vec<String>,
    /// CVEs a patch in `pkgs/<name>/patches/` fixes, by file.
    patched: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Hit {
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub sources: BTreeSet<String>,
    /// `affected` (a source says this version is), or `unknown` (a record
    /// names the product with no version it can be held to).
    pub status: String,
    pub kev: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epss: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub severity: String,
    pub summary: String,
    /// Versions a source says fix it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed: Vec<String>,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PkgReport {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpe: Option<String>,
    pub hits: Vec<Hit>,
    /// CVE id: why it is left out (`not_affected`, or the patch that fixes it).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub suppressed: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub generated: String,
    pub commit: String,
    /// Each source: `ok …` or the error that kept it out.
    pub sources: BTreeMap<String, String>,
    pub packages: Vec<PkgReport>,
}

pub fn run(repo: &Path, opts: &Opts) -> Result<()> {
    let targets = targets(repo, opts.name.as_deref())?;
    let mut report = Report {
        generated: oath_core::now_rfc3339(),
        commit: crate::util::run_out(
            Command::new("git").arg("-C").arg(repo).args(["rev-parse", "HEAD"]),
        )
        .unwrap_or_default(),
        ..Default::default()
    };
    let mut hits: BTreeMap<String, BTreeMap<String, Hit>> = BTreeMap::new();
    let mut add = |pkg: &str, h: Hit| merge(hits.entry(pkg.to_string()).or_default(), h);

    let mirror = cvelist_dir();
    match cve5(&mirror, &targets, opts.offline) {
        Ok((n, found)) => {
            report.sources.insert("cve.org (CVE List V5)".into(), format!("ok, {n} records"));
            for (pkg, h) in found {
                add(&pkg, h);
            }
        }
        Err(e) => {
            report.sources.insert("cve.org (CVE List V5)".into(), format!("error: {e:#}"));
        }
    }
    match osv(&targets) {
        Ok((n, found)) => {
            report.sources.insert("OSV".into(), format!("ok, {n} queries"));
            for (pkg, h) in found {
                add(&pkg, h);
            }
        }
        Err(e) => {
            report.sources.insert("OSV".into(), format!("error: {e:#}"));
        }
    }
    if opts.offline {
        report.sources.insert("NVD".into(), "skipped (--offline)".into());
    } else {
        match nvd(&targets) {
            Ok((n, found)) => {
                report.sources.insert("NVD".into(), format!("ok, {n} CPEs"));
                for (pkg, h) in found {
                    add(&pkg, h);
                }
            }
            Err(e) => {
                report.sources.insert("NVD".into(), format!("error: {e:#}"));
            }
        }
    }
    let kev = match kev() {
        Ok(k) => {
            report.sources.insert("CISA KEV".into(), format!("ok, {} CVEs", k.len()));
            k
        }
        Err(e) => {
            report.sources.insert("CISA KEV".into(), format!("error: {e:#}"));
            BTreeSet::new()
        }
    };
    let ids: BTreeSet<String> = hits
        .values()
        .flat_map(|m| m.values())
        .flat_map(|h| std::iter::once(h.id.clone()).chain(h.aliases.clone()))
        .filter(|i| i.starts_with("CVE-"))
        .collect();
    let epss = match epss(&ids) {
        Ok(e) => {
            report.sources.insert("EPSS".into(), format!("ok, {} scores", e.len()));
            e
        }
        Err(e) => {
            report.sources.insert("EPSS".into(), format!("error: {e:#}"));
            BTreeMap::new()
        }
    };

    for t in &targets {
        let mut p = PkgReport {
            name: t.name.clone(),
            version: t.version.clone(),
            cpe: t.entry.cpe.clone(),
            skip: t.entry.skip.clone(),
            ..Default::default()
        };
        for (_, mut h) in hits.remove(&t.name).unwrap_or_default() {
            let names: Vec<String> =
                std::iter::once(h.id.clone()).chain(h.aliases.clone()).collect();
            if let Some(why) = names.iter().find_map(|i| {
                t.entry.not_affected.get(i).map(|w| format!("not affected: {w}"))
                    .or_else(|| t.patched.get(i).map(|f| format!("patched: {f}")))
            }) {
                p.suppressed.insert(h.id.clone(), why);
                continue;
            }
            h.kev = names.iter().any(|i| kev.contains(i));
            h.epss = names.iter().find_map(|i| epss.get(i).copied());
            p.hits.push(h);
        }
        p.hits.sort_by(|a, b| rank(b).partial_cmp(&rank(a)).unwrap_or(Ordering::Equal));
        report.packages.push(p);
    }
    let dir = repo.join("build/audit");
    fs::create_dir_all(&dir)?;
    let day = report.generated.get(..10).unwrap_or("today").to_string();
    crate::util::write_pretty(&dir.join(format!("{day}.json")), &serde_json::to_value(&report)?)?;
    crate::util::write_pretty(&dir.join("latest.json"), &serde_json::to_value(&report)?)?;
    print_summary(&report);
    println!("report: {}", dir.join(format!("{day}.json")).display());
    Ok(())
}

/// KEV first, then affected over unknown, then EPSS.
fn rank(h: &Hit) -> f64 {
    let affected = if h.status == "affected" { 2.0 } else { 0.0 };
    (h.kev as u8 as f64) * 10.0 + affected + h.epss.unwrap_or(0.0)
}

fn print_summary(r: &Report) {
    for (s, st) in &r.sources {
        println!("{s}: {st}");
    }
    let mut any = false;
    for p in &r.packages {
        let affected = p.hits.iter().filter(|h| h.status == "affected").count();
        let unknown = p.hits.len() - affected;
        if p.hits.is_empty() {
            continue;
        }
        any = true;
        let kev = p.hits.iter().filter(|h| h.kev).count();
        let top: Vec<&str> = p.hits.iter().take(3).map(|h| h.id.as_str()).collect();
        println!(
            "pkg:{} {}: {affected} affected, {unknown} unknown{} ({})",
            p.name,
            p.version,
            if kev > 0 { format!(", {kev} known exploited") } else { String::new() },
            top.join(", ")
        );
    }
    if !any {
        println!("no package has a hit");
    }
}

fn merge(m: &mut BTreeMap<String, Hit>, h: Hit) {
    // One CVE, whichever id a source gave it.
    let key = std::iter::once(&h.id)
        .chain(h.aliases.iter())
        .find(|i| i.starts_with("CVE-"))
        .cloned()
        .unwrap_or_else(|| h.id.clone());
    match m.get_mut(&key) {
        Some(cur) => {
            cur.sources.extend(h.sources);
            for a in std::iter::once(h.id).chain(h.aliases) {
                if a != cur.id && !cur.aliases.contains(&a) {
                    cur.aliases.push(a);
                }
            }
            if h.status == "affected" {
                cur.status = h.status;
            }
            if cur.summary.is_empty() {
                cur.summary = h.summary;
            }
            if cur.severity.is_empty() {
                cur.severity = h.severity;
            }
            for f in h.fixed {
                if !cur.fixed.contains(&f) {
                    cur.fixed.push(f);
                }
            }
        }
        None => {
            let mut h = h;
            if h.id != key {
                let old = std::mem::replace(&mut h.id, key.clone());
                h.aliases.retain(|a| *a != key);
                h.aliases.push(old);
                h.url = format!("https://www.cve.org/CVERecord?id={key}");
            }
            m.insert(key, h);
        }
    }
}

/// `pkgs/audit.toml`.
pub fn entries(repo: &Path) -> Result<BTreeMap<String, Entry>> {
    let p = crate::pkgs::pkgs_dir(repo).join("audit.toml");
    let text = fs::read_to_string(&p).with_context(|| p.display().to_string())?;
    toml::from_str(&text).with_context(|| format!("parse {}", p.display()))
}

fn targets(repo: &Path, only: Option<&str>) -> Result<Vec<Target>> {
    let pkgs = crate::pkgs::load(repo)?;
    let entries = entries(repo)?;
    let lock = crate::pkgs::read_lock(repo)?;
    let cache = crate::pack::host_store_cache(repo);
    let only = only.map(|o| o.trim_start_matches("pkg:").to_string());
    let mut out = Vec::new();
    for (name, p) in &pkgs {
        if p.i386.is_some() || only.as_ref().is_some_and(|o| o != name) {
            continue;
        }
        let Some(entry) = entries.get(name).cloned() else {
            bail!("pkgs/{name} has no entry in pkgs/audit.toml: add one (cpe, or skip and why)");
        };
        let version = match &entry.version_after {
            Some(m) => p.version.split(m.as_str()).nth(1).unwrap_or(&p.version).to_string(),
            None => p.version.clone(),
        };
        let product = lock.get(name).map(|e| e.product.clone()).unwrap_or_default();
        let purls = ecosystem_purls(&cache.join("pkg").join(name).join(&product));
        out.push(Target {
            name: name.clone(),
            version,
            entry,
            purls,
            patched: patched(&crate::pkgs::pkgs_dir(repo).join(name).join("patches")),
        });
    }
    // pkg:oath, which cargo builds from this tree: its crates.
    if only.as_deref().is_none_or(|o| o == "oath") {
        let product = fs::read_to_string(repo.join("build/pkgs/oath.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["product"].as_str().map(str::to_string))
            .unwrap_or_default();
        out.push(Target {
            name: "oath".into(),
            version: "this tree".into(),
            entry: Entry {
                skip: Some("Oath's own: its crates go to OSV".into()),
                ..Default::default()
            },
            purls: ecosystem_purls(&cache.join("pkg/oath").join(product)),
            patched: BTreeMap::new(),
        });
    }
    Ok(out)
}

fn ecosystem_purls(tree: &Path) -> Vec<String> {
    let Some(comps) = oath_core::read_components(tree) else { return Vec::new() };
    comps
        .into_iter()
        .map(|c| c.purl)
        .filter(|p| {
            p.strip_prefix("pkg:")
                .and_then(|r| r.split('/').next())
                .is_some_and(|t| ECOSYSTEMS.contains(&t))
        })
        .collect()
}

/// CVE id: patch file, for each `*CVE-YYYY-N*.patch` in `dir`.
fn patched(dir: &Path) -> BTreeMap<String, String> {
    let re = regex::Regex::new(r"CVE-\d{4}-\d{4,}").expect("regex");
    let mut out = BTreeMap::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let f = e.file_name().to_string_lossy().into_owned();
        for m in re.find_iter(&f) {
            out.insert(m.as_str().to_string(), f.clone());
        }
    }
    out
}

/// `{version}` and `{version_}` (dots as `_`) in a tag pattern.
fn tag_of(pattern: &str, version: &str) -> String {
    pattern.replace("{version_}", &version.replace('.', "_")).replace("{version}", version)
}

/// The CPE NVD matches for this version: OpenSSH's `10.6p1` is version
/// `10.6`, update `p1`.
fn cpe_with_version(cpe: &str, version: &str) -> String {
    let re = regex::Regex::new(r"^(\d+(?:\.\d+)*)(p\d+)$").expect("regex");
    match re.captures(version) {
        Some(c) => format!("{cpe}:{}:{}", &c[1], &c[2]),
        None => format!("{cpe}:{version}"),
    }
}

// ---------- versions ----------

#[derive(Debug, PartialEq, Eq)]
enum Tok {
    Num(u128),
    Word(String),
    Any,
}

fn toks(v: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<Tok>| {
        if cur.is_empty() {
            return;
        }
        if cur.bytes().all(|b| b.is_ascii_digit()) {
            out.push(Tok::Num(cur.parse().unwrap_or(u128::MAX)));
        } else {
            out.push(Tok::Word(cur.to_ascii_lowercase()));
        }
        cur.clear();
    };
    for ch in v.trim().trim_start_matches(['v', 'V']).chars() {
        if ch == '*' {
            flush(&mut cur, &mut out);
            out.push(Tok::Any);
        } else if ch.is_ascii_alphanumeric() {
            let switch =
                cur.chars().last().is_some_and(|l| l.is_ascii_digit() != ch.is_ascii_digit());
            if switch {
                flush(&mut cur, &mut out);
            }
            cur.push(ch);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// A release after the same version (`p1`, OpenSSH's portable patches).
fn post(w: &str) -> bool {
    matches!(w, "p" | "pl" | "patch" | "post")
}

/// Upstream version order: numbers by value, `rc`/`beta` before the
/// release, `p1` after it, `*` above anything.
pub fn cmp_versions(a: &str, b: &str) -> Ordering {
    let (a, b) = (toks(a), toks(b));
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (Some(Tok::Any), _) | (_, Some(Tok::Any)) => return Ordering::Equal,
            (Some(Tok::Num(x)), Some(Tok::Num(y))) if x != y => return x.cmp(y),
            (Some(Tok::Word(x)), Some(Tok::Word(y))) if x != y => return x.cmp(y),
            (Some(Tok::Num(_)), Some(Tok::Word(w))) => {
                return if post(w) { Ordering::Less } else { Ordering::Greater }
            }
            (Some(Tok::Word(w)), Some(Tok::Num(_))) => {
                return if post(w) { Ordering::Greater } else { Ordering::Less }
            }
            (Some(_), None) => {
                return match &a[i] {
                    Tok::Word(w) if !post(w) => Ordering::Less,
                    _ => Ordering::Greater,
                }
            }
            (None, Some(_)) => {
                return match &b[i] {
                    Tok::Word(w) if !post(w) => Ordering::Greater,
                    _ => Ordering::Less,
                }
            }
            _ => {}
        }
    }
    Ordering::Equal
}

/// What a CVE record's `affected` entry says of `version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Says {
    /// A range in it holds the version: `affected` or `unaffected`.
    Range(&'static str),
    /// No range holds it: what the entry means outside its ranges.
    Default(&'static str),
}

/// A range's bound written as text (`< 1.77.2`, `<= 2.0`), as some CNAs do.
fn text_bound(v: &str) -> Option<(&str, bool)> {
    let v = v.trim();
    if let Some(x) = v.strip_prefix("<=") {
        return Some((x.trim(), true));
    }
    v.strip_prefix('<').map(|x| (x.trim(), false))
}

pub fn status_of(entry: &Value, version: &str) -> Says {
    let declared = match entry["defaultStatus"].as_str() {
        Some("affected") => "affected",
        Some("unaffected") => "unaffected",
        _ => "unknown",
    };
    // A product named with no versions says nothing of this release.
    let versions = match entry["versions"].as_array() {
        Some(v) if !v.is_empty() => v,
        _ if declared == "unaffected" => return Says::Default("unaffected"),
        _ => return Says::Default("unknown"),
    };
    let (mut ranged, mut lists_unaffected) = (false, false);
    for v in versions {
        // A commit is no version to compare (CISA's "custom" ranges of the
        // kernel's abbreviated commits): OSV's commit query judges those.
        let commit = ["version", "lessThan", "lessThanOrEqual"]
            .iter()
            .any(|k| v[*k].as_str().is_some_and(commit_like));
        if v["versionType"].as_str() == Some("git") || commit {
            continue;
        }
        let status = match v["status"].as_str() {
            Some("affected") => "affected",
            Some("unaffected") => "unaffected",
            _ => continue,
        };
        let raw = v["version"].as_str().unwrap_or("");
        let (lo, text_hi) = match text_bound(raw) {
            Some((hi, eq)) => ("0", Some((hi, eq))),
            None => (raw, None),
        };
        let from_start = matches!(lo, "" | "0" | "*" | "n/a" | "unspecified");
        let at_least = from_start || cmp_versions(version, lo) != Ordering::Less;
        let below = |hi: &str, eq: bool| {
            hi == "*" || {
                let o = cmp_versions(version, hi);
                o == Ordering::Less || eq && o == Ordering::Equal
            }
        };
        let inside = if let Some(hi) = v["lessThan"].as_str() {
            at_least && below(hi, false)
        } else if let Some(hi) = v["lessThanOrEqual"].as_str() {
            at_least && below(hi, true)
        } else if let Some((hi, eq)) = text_hi {
            below(hi, eq)
        } else if from_start {
            continue;
        } else {
            cmp_versions(version, lo) == Ordering::Equal
        };
        ranged = true;
        lists_unaffected |= status == "unaffected";
        if inside {
            return Says::Range(status);
        }
    }
    // Ranges that list only what is affected say the rest is not, whatever
    // the default says (CISA's added entries mark every one `affected`);
    // with unaffected ranges among them (the kernel's), the default holds.
    match (ranged, lists_unaffected, declared) {
        (true, false, _) => Says::Default("unaffected"),
        (true, true, "unknown") => Says::Default("unaffected"),
        (_, _, d) => Says::Default(d),
    }
}

/// A git commit id, whole or abbreviated (`c460495ee072`), not a version.
fn commit_like(s: &str) -> bool {
    (7..=40).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_hexdigit())
        && s.bytes().any(|b| b.is_ascii_alphabetic())
}

/// One verdict from every entry that matched: a range that holds the
/// version wins over a default, and affected over unaffected.
fn verdict(says: &[Says]) -> Option<&'static str> {
    let has = |s: Says| says.contains(&s);
    if has(Says::Range("affected")) {
        Some("affected")
    } else if has(Says::Range("unaffected")) {
        None
    } else if has(Says::Default("affected")) {
        Some("affected")
    } else if has(Says::Default("unknown")) {
        Some("unknown")
    } else {
        None
    }
}

// ---------- CVE List V5 ----------

fn cvelist_dir() -> PathBuf {
    std::env::var_os("OATH_CVELIST").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/home".into());
        home.join(".local/share/oath/cvelist")
    })
}

fn norm(s: &str) -> String {
    s.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

const NO_VENDOR: &[&str] = &["", "na", "unknown", "unspecified", "notavailable"];

/// Product names too common to trust without the vendor.
const GENERIC: &[&str] = &[
    "bc", "make", "less", "tar", "sed", "grep", "flex", "patch", "m4", "gzip", "git", "glib",
    "mesa", "node", "rust", "python", "perl", "steam", "linux", "cli", "codex", "chrome",
];

struct Matcher {
    cpe: Option<String>,
    vendors: BTreeSet<String>,
    products: BTreeSet<String>,
    repo: Option<String>,
}

impl Matcher {
    fn of(t: &Target) -> Self {
        let mut vendors = BTreeSet::new();
        let mut products = BTreeSet::new();
        let mut cpe = None;
        if let Some(c) = &t.entry.cpe {
            let parts: Vec<&str> = c.split(':').collect();
            if let (Some(v), Some(p)) = (parts.get(3), parts.get(4)) {
                vendors.insert(norm(v));
                vendors.insert(norm(v.trim_end_matches("_project")));
                products.insert(norm(p));
                cpe = Some(format!("cpe:2.3:a:{v}:{p}:").to_ascii_lowercase());
            }
        }
        products.insert(norm(&t.name));
        for n in &t.entry.names {
            products.insert(norm(n));
        }
        let repo = t.entry.repo.as_ref().map(|r| r.trim_end_matches(".git").to_ascii_lowercase());
        Self { cpe, vendors, products, repo }
    }

    fn matches(&self, a: &Value) -> bool {
        let cpes = a["cpes"].as_array().into_iter().flatten().filter_map(Value::as_str);
        if let Some(c) = &self.cpe {
            if cpes.clone().any(|x| x.to_ascii_lowercase().starts_with(c.as_str())) {
                return true;
            }
        }
        if let Some(r) = &self.repo {
            let repo =
                a["repo"].as_str().unwrap_or("").trim_end_matches(".git").to_ascii_lowercase();
            if !repo.is_empty() && repo == *r {
                return true;
            }
        }
        let product = norm(a["product"].as_str().unwrap_or(""));
        if product.is_empty() || !self.products.contains(&product) {
            return false;
        }
        let vendor = norm(a["vendor"].as_str().unwrap_or(""));
        self.vendors.contains(&vendor)
            || self.products.contains(&vendor)
            || NO_VENDOR.contains(&vendor.as_str()) && !GENERIC.contains(&product.as_str())
    }
}

fn update_mirror(dir: &Path) -> Result<()> {
    let git =
        |args: &[&str]| crate::util::run_out(Command::new("git").arg("-C").arg(dir).args(args));
    if dir.join(".git").is_dir() {
        git(&["fetch", "-q", "--depth", "1", "origin", "main"]).context("git fetch cvelistV5")?;
        git(&["reset", "-q", "--hard", "FETCH_HEAD"])?;
    } else {
        fs::create_dir_all(dir.parent().context("parent")?)?;
        crate::util::run_out(
            Command::new("git").args(["clone", "-q", "--depth", "1", CVELIST]).arg(dir),
        )
        .context("git clone cvelistV5")?;
    }
    Ok(())
}

fn cve5(dir: &Path, targets: &[Target], offline: bool) -> Result<(usize, Vec<(String, Hit)>)> {
    if !offline || !dir.join("cves").is_dir() {
        update_mirror(dir)?;
    }
    let matchers: Vec<(usize, Matcher)> = targets
        .iter()
        .enumerate()
        .filter(|(_, t)| t.entry.skip.is_none())
        .map(|(i, t)| (i, Matcher::of(t)))
        .collect();
    // A file can match only if it names a product or CPE of ours.
    let mut words: BTreeSet<String> = BTreeSet::new();
    for (i, m) in &matchers {
        words.extend(targets[*i].entry.names.iter().cloned());
        words.insert(targets[*i].name.clone());
        if let Some(c) = &m.cpe {
            words.insert(c.split(':').nth(4).unwrap_or("").replace('_', " "));
            words.insert(c.split(':').nth(4).unwrap_or("").to_string());
        }
    }
    let alt: Vec<String> = words.iter().filter(|w| w.len() > 1).map(|w| regex::escape(w)).collect();
    let cpes: Vec<String> =
        matchers.iter().filter_map(|(_, m)| m.cpe.as_deref()).map(regex::escape).collect();
    let pre = regex::bytes::RegexBuilder::new(&format!(
        r#""product"\s*:\s*"[^"]*(?:{})|(?:{})"#,
        alt.join("|"),
        cpes.join("|")
    ))
        .case_insensitive(true)
        .size_limit(1 << 26)
        .build()
        .context("prefilter")?;
    let mut n = 0usize;
    let mut out = Vec::new();
    let mut stack = vec![dir.join("cves")];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            n += 1;
            let Ok(bytes) = fs::read(&p) else { continue };
            if !pre.is_match(&bytes) {
                continue;
            }
            let Ok(rec) = serde_json::from_slice::<Value>(&bytes) else { continue };
            if rec["cveMetadata"]["state"].as_str() != Some("PUBLISHED") {
                continue;
            }
            let entries = |c: &Value| -> Vec<Value> {
                c["affected"].as_array().cloned().unwrap_or_default()
            };
            let cna = entries(&rec["containers"]["cna"]);
            let adp = rec["containers"]["adp"].as_array().into_iter().flatten();
            let adp: Vec<Value> = adp.flat_map(entries).collect();
            for (i, m) in &matchers {
                let t = &targets[*i];
                let judge = |list: &[Value]| -> Vec<Says> {
                    list.iter().filter(|a| m.matches(a)).map(|a| status_of(a, &t.version)).collect()
                };
                // The CNA speaks for its product; CISA's added entries (which
                // sometimes mark the kernel's fixed ranges affected) count
                // only where it holds no range for this version.
                let mut says = judge(&cna);
                if !says.iter().any(|s| matches!(s, Says::Range(_))) {
                    says.extend(judge(&adp));
                }
                if says.is_empty() {
                    continue;
                }
                let Some(s) = verdict(&says) else { continue };
                out.push((t.name.clone(), cve5_hit(&rec, s)));
            }
        }
    }
    Ok((n, out))
}

fn cve5_hit(rec: &Value, status: &str) -> Hit {
    let id = rec["cveMetadata"]["cveId"].as_str().unwrap_or("").to_string();
    let cna = &rec["containers"]["cna"];
    let summary = cna["descriptions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["lang"].as_str().is_some_and(|l| l.starts_with("en")))
        .and_then(|d| d["value"].as_str())
        .unwrap_or("");
    let severity = std::iter::once(cna)
        .chain(rec["containers"]["adp"].as_array().into_iter().flatten())
        .flat_map(|c| c["metrics"].as_array().into_iter().flatten())
        .find_map(|m| {
            ["cvssV4_0", "cvssV3_1", "cvssV3_0"]
                .iter()
                .find_map(|k| m[*k]["baseSeverity"].as_str().map(str::to_string))
        })
        .unwrap_or_default();
    Hit {
        url: format!("https://www.cve.org/CVERecord?id={id}"),
        id,
        sources: BTreeSet::from(["cve.org".to_string()]),
        status: status.into(),
        severity,
        summary: short(summary),
        ..Default::default()
    }
}

fn short(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.len() <= 300 {
        return s;
    }
    let mut end = 300;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

// ---------- OSV ----------

fn osv(targets: &[Target]) -> Result<(usize, Vec<(String, Hit)>)> {
    let mut queries = Vec::new();
    let mut whose = Vec::new();
    for t in targets {
        for p in &t.purls {
            queries.push(json!({ "package": { "purl": p } }));
            whose.push(t.name.clone());
        }
        if let Some(o) = &t.entry.osv {
            let (eco, name) = o.split_once(':').context("osv is <ecosystem>:<name>")?;
            let package = json!({ "ecosystem": eco, "name": name });
            queries.push(json!({ "package": package, "version": t.version }));
            whose.push(t.name.clone());
        }
        if let (Some(r), Some(tag)) = (&t.entry.repo, &t.entry.tag) {
            if let Some(c) = tag_commit(r, &tag_of(tag, &t.version)) {
                queries.push(json!({ "commit": c }));
                whose.push(t.name.clone());
            }
        }
    }
    let mut ids: Vec<(String, String)> = Vec::new();
    for (chunk, owners) in queries.chunks(500).zip(whose.chunks(500)) {
        let body = json!({ "queries": chunk }).to_string();
        let raw = http(&format!("{OSV}/querybatch"), Some(&body), None)?;
        let v: Value = serde_json::from_str(&raw).context("OSV querybatch JSON")?;
        for (r, owner) in v["results"].as_array().into_iter().flatten().zip(owners) {
            for vu in r["vulns"].as_array().into_iter().flatten() {
                if let Some(id) = vu["id"].as_str() {
                    ids.push((owner.clone(), id.to_string()));
                }
            }
        }
    }
    let mut details: BTreeMap<String, Value> = BTreeMap::new();
    let mut out = Vec::new();
    for (owner, id) in ids {
        if !details.contains_key(&id) {
            let raw = http(&format!("{OSV}/vulns/{id}"), None, None)?;
            details.insert(id.clone(), serde_json::from_str(&raw).unwrap_or(Value::Null));
        }
        let d = &details[&id];
        let fixed: Vec<String> = d["affected"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|a| a["ranges"].as_array().into_iter().flatten())
            .flat_map(|r| r["events"].as_array().into_iter().flatten())
            .filter_map(|e| e["fixed"].as_str().map(str::to_string))
            .filter(|f| f.len() < 40)
            .collect();
        out.push((
            owner,
            Hit {
                id: id.clone(),
                aliases: d["aliases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect(),
                sources: BTreeSet::from(["OSV".to_string()]),
                status: "affected".into(),
                summary: short(d["summary"].as_str().or(d["details"].as_str()).unwrap_or("")),
                fixed,
                url: format!("https://osv.dev/vulnerability/{id}"),
                ..Default::default()
            },
        ));
    }
    Ok((queries.len(), out))
}

/// The commit a release tag names (its peeled commit for an annotated tag).
fn tag_commit(repo: &str, tag: &str) -> Option<String> {
    let out = crate::util::run_out(
        Command::new("git").args([
            "ls-remote",
            repo,
            &format!("refs/tags/{tag}"),
            &format!("refs/tags/{tag}^{{}}"),
        ]),
    )
    .ok()?;
    let mut best = None;
    for line in out.lines() {
        let (sha, r) = line.split_once('\t')?;
        if r.ends_with("^{}") || best.is_none() {
            best = Some(sha.to_string());
        }
    }
    best
}

// ---------- NVD ----------

fn nvd(targets: &[Target]) -> Result<(usize, Vec<(String, Hit)>)> {
    let key = std::env::var("NVD_API_KEY").ok().or_else(|| env_file("nvd.env", "NVD_API_KEY"));
    let pause = std::time::Duration::from_millis(if key.is_some() { 700 } else { 6500 });
    let mut out = Vec::new();
    let mut n = 0;
    let mut errors = Vec::new();
    for t in targets.iter().filter(|t| t.entry.skip.is_none()) {
        let Some(cpe) = &t.entry.cpe else { continue };
        n += 1;
        let url = format!("{NVD}?virtualMatchString={}", cpe_with_version(cpe, &t.version));
        let header = key.as_ref().map(|k| format!("apiKey: {k}"));
        let raw = match http(&url, None, header.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("{}: {e:#}", t.name));
                std::thread::sleep(pause);
                continue;
            }
        };
        std::thread::sleep(pause);
        let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        for item in v["vulnerabilities"].as_array().into_iter().flatten() {
            let c = &item["cve"];
            let id = c["id"].as_str().unwrap_or("").to_string();
            let summary = c["descriptions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["lang"].as_str() == Some("en"))
                .and_then(|d| d["value"].as_str())
                .unwrap_or("");
            let severity = ["cvssMetricV40", "cvssMetricV31", "cvssMetricV30"]
                .iter()
                .find_map(|k| c["metrics"][*k][0]["cvssData"]["baseSeverity"].as_str())
                .unwrap_or("")
                .to_string();
            out.push((
                t.name.clone(),
                Hit {
                    url: format!("https://nvd.nist.gov/vuln/detail/{id}"),
                    id,
                    sources: BTreeSet::from(["NVD".to_string()]),
                    status: "affected".into(),
                    severity,
                    summary: short(summary),
                    ..Default::default()
                },
            ));
        }
    }
    if !errors.is_empty() && out.is_empty() && n > 0 {
        bail!("{} CPEs failed: {}", errors.len(), errors.join("; "));
    }
    Ok((n, out))
}

/// `NAME=value` from `~/.config/oath/<file>` (never committed).
fn env_file(file: &str, name: &str) -> Option<String> {
    let text = fs::read_to_string(crate::origin::config_file().with_file_name(file)).ok()?;
    text.lines().find_map(|l| l.strip_prefix(&format!("{name}=")).map(|v| v.trim().to_string()))
}

// ---------- KEV, EPSS ----------

fn kev() -> Result<BTreeSet<String>> {
    let v: Value = serde_json::from_str(&http(KEV, None, None)?).context("KEV JSON")?;
    Ok(v["vulnerabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["cveID"].as_str().map(str::to_string))
        .collect())
}

fn epss(ids: &BTreeSet<String>) -> Result<BTreeMap<String, f64>> {
    let ids: Vec<&String> = ids.iter().collect();
    let mut out = BTreeMap::new();
    for chunk in ids.chunks(80) {
        let list: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
        let raw = http(&format!("{EPSS}?cve={}", list.join(",")), None, None)?;
        let v: Value = serde_json::from_str(&raw).context("EPSS JSON")?;
        for d in v["data"].as_array().into_iter().flatten() {
            if let (Some(c), Some(e)) = (d["cve"].as_str(), d["epss"].as_str()) {
                out.insert(c.to_string(), e.parse().unwrap_or(0.0));
            }
        }
    }
    Ok(out)
}

// ---------- HTTP ----------

/// GET, or POST `body` as JSON; `header` goes to curl on stdin (an API
/// key never reaches argv).
fn http(url: &str, body: Option<&str>, header: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-f", "-L", "--retry", "2", "--max-time", "180", "-K", "-"]);
    let mut config = format!("url = \"{url}\"\nuser-agent = \"oath-audit\"\n");
    if let Some(h) = header {
        config.push_str(&format!("header = \"{h}\"\n"));
    }
    if let Some(b) = body {
        config.push_str("header = \"Content-Type: application/json\"\n");
        let f = std::env::temp_dir().join(format!("oath-audit-{}.json", std::process::id()));
        fs::write(&f, b)?;
        config.push_str(&format!("data-binary = \"@{}\"\n", f.display()));
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("curl")?;
    child.stdin.take().context("stdin")?.write_all(config.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_as_upstreams_mean() {
        use Ordering::*;
        assert_eq!(cmp_versions("8.23.0", "8.24.0"), Less);
        assert_eq!(cmp_versions("2.10.0", "2.9.0"), Greater);
        assert_eq!(cmp_versions("1.0rc1", "1.0"), Less);
        assert_eq!(cmp_versions("10.6p1", "10.6"), Greater);
        assert_eq!(cmp_versions("10.6p1", "10.7"), Less);
        assert_eq!(cmp_versions("7.3", "7.3.*"), Equal);
        assert_eq!(cmp_versions("v1.2.3", "1.2.3"), Equal);
        assert_eq!(cmp_versions("20250605", "20240905"), Greater);
    }

    #[test]
    fn a_record_says_whether_a_version_is_affected() {
        use Says::*;
        let curl = json!({ "vendor": "curl", "product": "curl", "defaultStatus": "unaffected",
            "versions": [{ "version": "7.69.0", "lessThan": "8.24.0", "status": "affected", "versionType": "semver" }] });
        assert_eq!(status_of(&curl, "8.23.0"), Range("affected"));
        assert_eq!(status_of(&curl, "8.24.0"), Default("unaffected"));
        // The kernel's CNA: affected but for the stable ranges that fix it.
        let linux = json!({ "vendor": "Linux", "product": "Linux", "defaultStatus": "affected",
            "versions": [
                { "version": "6.1", "status": "affected" },
                { "version": "0", "lessThan": "6.1", "status": "unaffected", "versionType": "semver" },
                { "version": "7.3.4", "lessThanOrEqual": "7.3.*", "status": "unaffected", "versionType": "semver" },
                { "version": "7.4", "lessThanOrEqual": "*", "status": "unaffected", "versionType": "original_commit_for_fix" }
            ] });
        assert_eq!(status_of(&linux, "7.3"), Default("affected"));
        assert_eq!(status_of(&linux, "7.3.5"), Range("unaffected"));
        assert_eq!(status_of(&linux, "7.4"), Range("unaffected"));
        assert_eq!(status_of(&linux, "6.0"), Range("unaffected"));
        // CISA's added entry: an affected range, every one defaulting to
        // affected; Chrome 156 is past it (CVE-2024-5160).
        let chrome = json!({ "product": "chrome", "defaultStatus": "affected",
            "versions": [{ "status": "affected", "version": "0", "lessThan": "125.0.6422.76", "versionType": "custom" }] });
        assert_eq!(status_of(&chrome, "156.0.8078.48"), Default("unaffected"));
        // A bound written as text (CVE-2024-24576's CNA).
        let rust = json!({ "product": "rust", "versions": [{ "version": "< 1.77.2", "status": "affected" }] });
        assert_eq!(status_of(&rust, "1.99.1"), Default("unaffected"));
        assert_eq!(status_of(&rust, "1.77.1"), Range("affected"));
        // Only git ranges, no default: nothing to hold the version to.
        let git = json!({ "product": "x", "versions": [{ "version": "abc", "lessThan": "def", "status": "affected", "versionType": "git" }] });
        assert_eq!(status_of(&git, "1.0"), Default("unknown"));
        // A product with no versions (CVE-2023-3354's `qemu`): unknown.
        let bare = json!({ "product": "qemu", "defaultStatus": "affected" });
        assert_eq!(status_of(&bare, "11.1.2"), Default("unknown"));
        // An exact version.
        let exact = json!({ "product": "x", "versions": [{ "version": "2.9.0", "status": "affected" }] });
        assert_eq!(status_of(&exact, "2.9.0"), Range("affected"));
        assert_eq!(status_of(&exact, "2.10.0"), Default("unaffected"));
        // CVE-2024-36971: CISA split the kernel's ranges into one entry each,
        // all defaulting to affected; the one whose range holds 7.3 decides.
        let split = [
            json!({ "defaultStatus": "affected", "versions": [{ "status": "unaffected", "version": "6.9.4", "lessThan": "6.10" }] }),
            json!({ "defaultStatus": "affected", "versions": [{ "status": "unaffected", "version": "6.10", "lessThanOrEqual": "*" }] }),
        ];
        let says: Vec<Says> = split.iter().map(|e| status_of(e, "7.3")).collect();
        assert_eq!(verdict(&says), None);
        // Commits in a "custom" range (CVE-2024-27053's added entries,
        // CVE-2022-42896's CNA) hold no version.
        let commits = json!({ "versions": [{ "version": "c460495ee072", "status": "affected", "lessThan": "5800ec78775c", "versionType": "custom" }] });
        assert_eq!(status_of(&commits, "7.3"), Default("unknown"));
        let google = json!({ "defaultStatus": "unaffected", "versions": [{ "lessThanOrEqual": "711f8c3fb3db61897080468586b970c87c61d9e4", "status": "affected", "version": "3.0.0", "versionType": "custom" }] });
        assert_eq!(status_of(&google, "7.3"), Default("unaffected"));
        assert!(!commit_like("20250605") && !commit_like("1.0.0") && commit_like("4bfd20d5f5c"));
        assert_eq!(verdict(&[Default("affected"), Default("unknown")]), Some("affected"));
        assert_eq!(verdict(&[Default("unknown")]), Some("unknown"));
    }

    #[test]
    fn records_match_by_cpe_product_and_repo() {
        let t = |name: &str, cpe: &str, names: &[&str]| Target {
            name: name.into(),
            entry: Entry {
                cpe: Some(cpe.into()),
                names: names.iter().map(|s| s.to_string()).collect(),
                repo: Some("https://github.com/libexpat/libexpat".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let expat = Matcher::of(&t("expat", "cpe:2.3:a:libexpat_project:libexpat", &["expat"]));
        assert!(expat.matches(&json!({ "vendor": "libexpat project", "product": "libexpat" })));
        assert!(expat.matches(&json!({ "vendor": "n/a", "product": "Expat" })));
        assert!(expat.matches(&json!({ "product": "x", "cpes": ["cpe:2.3:a:libexpat_project:libexpat:*:*:*:*:*:*:*:*"] })));
        assert!(expat.matches(&json!({ "product": "x", "repo": "https://github.com/libexpat/libexpat.git" })));
        assert!(!expat.matches(&json!({ "vendor": "someone", "product": "libexpat-ruby" })));
        // A generic name needs its vendor.
        let patch = Matcher::of(&t("patch", "cpe:2.3:a:gnu:patch", &[]));
        assert!(!patch.matches(&json!({ "vendor": "n/a", "product": "patch" })));
        assert!(patch.matches(&json!({ "vendor": "GNU", "product": "patch" })));
    }

    #[test]
    fn tags_cpes_and_patches() {
        assert_eq!(tag_of("curl-{version_}", "8.23.0"), "curl-8_23_0");
        assert_eq!(tag_of("v{version}", "1.3.2"), "v1.3.2");
        assert_eq!(cpe_with_version("cpe:2.3:a:openbsd:openssh", "10.6p1"), "cpe:2.3:a:openbsd:openssh:10.6:p1");
        assert_eq!(cpe_with_version("cpe:2.3:a:haxx:curl", "8.23.0"), "cpe:2.3:a:haxx:curl:8.23.0");
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("oath-backport-CVE-2026-12345.patch"), "").unwrap();
        fs::write(d.path().join("upstream-CVE-2026-99999-fix.patch"), "").unwrap();
        let p = patched(d.path());
        assert_eq!(p["CVE-2026-12345"], "oath-backport-CVE-2026-12345.patch");
        assert!(p.contains_key("CVE-2026-99999"));
    }

    #[test]
    fn one_cve_merges_across_sources() {
        let mut m = BTreeMap::new();
        merge(&mut m, Hit { id: "GHSA-x".into(), aliases: vec!["CVE-2026-1".into()], sources: BTreeSet::from(["OSV".into()]), status: "affected".into(), ..Default::default() });
        merge(&mut m, Hit { id: "CVE-2026-1".into(), sources: BTreeSet::from(["cve.org".into()]), status: "unknown".into(), summary: "s".into(), ..Default::default() });
        assert_eq!(m.len(), 1);
        let h = &m["CVE-2026-1"];
        assert_eq!(h.id, "CVE-2026-1");
        assert_eq!(h.status, "affected");
        assert_eq!(h.sources.len(), 2);
        assert!(h.aliases.contains(&"GHSA-x".to_string()));
    }

    #[test]
    fn every_package_has_an_audit_entry() {
        let repo = crate::util::repo_root();
        let pkgs = crate::pkgs::load(&repo).unwrap();
        let e = entries(&repo).unwrap();
        for (n, p) in &pkgs {
            if p.i386.is_none() {
                assert!(e.contains_key(n), "pkgs/{n} has no entry in pkgs/audit.toml");
            }
        }
    }
}
