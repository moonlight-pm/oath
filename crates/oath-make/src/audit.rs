//! The daily security audit's scan (R48): every package this tree ships,
//! matched by `pkgs/audit.toml` against cve.org's CVE List V5 (a local
//! mirror of `CVEProject/cvelistV5`), OSV (ecosystem packages, and the
//! commit of an upstream's release tag), each GitHub repository's own
//! published advisories, NVD (by CPE) and CISA's Known Exploited
//! Vulnerabilities, with FIRST's EPSS beside each, and each pin against
//! upstream's newest stable release, for fixes no record names yet.
//! It writes `build/audit/<day>.json` and `build/audit/latest.json` for the
//! `oath-audit` skill to triage and changes nothing it scans. A source
//! that cannot be reached is recorded as such, and the rest still run.
//! `--publish` signs `latest.json` and puts it on the default origin, where
//! any machine reads it (`oath audit`, R61).

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
const GITHUB: &str = "https://api.github.com";
/// GitHub answers 60 unkeyed requests an hour: a run reads this many, and
/// leaves the rest of the hour to whoever runs it.
const GHSA_READS: usize = 50;
/// Purl types OSV knows (an upstream's `pkg:generic` and `pkg:github` it
/// does not; those go by CPE, product and commit).
const ECOSYSTEMS: &[&str] = &["cargo", "npm", "pypi", "golang", "gem", "maven", "nuget", "hex"];

pub struct Opts {
    /// Only this package (`curl` or `pkg:curl`).
    pub name: Option<String>,
    /// Use the CVE List mirror as it is, and skip NVD (no network but OSV,
    /// KEV and EPSS, which are quick).
    pub offline: bool,
    /// Only each pin against upstream's newest stable release; writes nothing.
    pub upstream: bool,
    /// Only put `latest.json` on the default origin, signed.
    pub publish: bool,
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
    /// The release line this package stays on: a newer line is no upgrade.
    pub series: Option<String>,
    /// An odd second number is a development line.
    #[serde(default)]
    pub dev_odd: bool,
    /// A third number from this on is a pre-release (Wayland's 1.26.90).
    pub dev_micro: Option<u64>,
    /// Why upstream's releases cannot be read here.
    pub unchecked: Option<String>,
    /// The page that lists upstream's releases, when the source's
    /// directory has no index of its own.
    pub page: Option<String>,
    /// Why the pin has no git tag to compare with (R54): a vendor's binaries,
    /// or files generated from the tree.
    pub no_compare: Option<String>,
    /// Why files in both the tarball and the tag differ, once someone has
    /// read them.
    pub differs_because: Option<String>,
    /// The repository to compare a pin with, where `repo` and `tag` are not
    /// set because upstream's listing is the better read of its releases.
    pub compare_repo: Option<String>,
    /// The tag there: `{version}`, `{version_}` (2_15_0), `{version-}`
    /// (2-15-0), or a tag's whole name where it holds a date or case.
    pub compare_tag: Option<String>,
    /// The sources are single files, each to be found in the tag's tree
    /// (firmware, fonts), not a tarball. `compare_tag` may name several tags.
    #[serde(default)]
    pub compare_files: bool,
    /// The key the tag must be signed by (`pkgs/keys/<key>`), for a pin that
    /// is not a tree of its own, so `git`, `tag` and `key` do not fit.
    pub compare_key: Option<String>,
    /// Why a package at a version, or the release (`source`), comes in under a
    /// week old (R57): the advisory it fixes.
    #[serde(default)]
    pub age_because: BTreeMap<String, String>,
    /// The tag's directory the tarball is made from (`expat`).
    pub tag_dir: Option<String>,
    /// The tarball's directory the tag's tree is in (`nss`).
    pub tar_dir: Option<String>,
    #[serde(default)]
    pub not_affected: BTreeMap<String, String>,
    /// A record's listed version is the newest affected one of its line, and
    /// every earlier one is affected too (Oracle's records of Java SE).
    #[serde(default)]
    pub listed_and_earlier: bool,
}

/// A package as the audit matches it.
#[derive(Clone, Debug, Default)]
struct Target {
    name: String,
    /// The upstream's version (`version_after` applied).
    version: String,
    /// The version the catalog shows for this pin.
    pin: String,
    /// The pin's tree, when it was built.
    hash: String,
    entry: Entry,
    /// Ecosystem purls in its pack's `components.json`.
    purls: Vec<String>,
    /// CVEs a patch in `pkgs/<name>/patches/` fixes, by file.
    patched: BTreeMap<String, String>,
    /// The directory its source comes from, and the source's name in it
    /// with `{version}`.
    listing: Option<(String, String)>,
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
    /// The version the catalog shows for this build.
    pub version: String,
    /// The build this read: another build of the version may lack the
    /// patches `suppressed` names.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// The version records were matched against, when `version` is not it
    /// (CEF's Chromium).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpe: Option<String>,
    pub hits: Vec<Hit>,
    /// CVE id: why it is left out (`not_affected`, or the patch that fixes it).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub suppressed: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,
    /// Upstream's newest stable release, when it is past this pin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newer: Option<String>,
    /// Why upstream's releases could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_unread: Option<String>,
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
    if opts.publish {
        return publish(repo);
    }
    let targets = targets(repo, opts.name.as_deref())?;
    if opts.upstream {
        let found = upstream(&targets);
        for t in &targets {
            let u = found.get(&t.name).cloned().unwrap_or_default();
            let (newer, unread) = (u.newer, u.unread);
            print_upstream(&t.name, &t.version, newer.as_deref(), unread.as_deref());
        }
        println!("upstream releases: {}", upstream_status(found.values()));
        return Ok(());
    }
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
    let (status, found) = ghsa(&targets, opts.offline);
    report.sources.insert("GitHub advisories".into(), status);
    for (pkg, h) in found {
        add(&pkg, h);
    }
    if opts.offline {
        report.sources.insert("NVD".into(), "skipped (--offline)".into());
    } else {
        match nvd(&targets) {
            Ok((status, found)) => {
                report.sources.insert("NVD".into(), status);
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
    let releases = upstream(&targets);
    report.sources.insert("Upstream releases".into(), upstream_status(releases.values()));

    for t in &targets {
        let mut p = PkgReport {
            name: t.name.clone(),
            version: t.pin.clone(),
            hash: t.hash.clone(),
            upstream: (t.version != t.pin).then(|| t.version.clone()),
            cpe: t.entry.cpe.clone(),
            skip: t.entry.skip.clone(),
            ..Default::default()
        };
        if let Some(u) = releases.get(&t.name) {
            p.newer.clone_from(&u.newer);
            p.upstream_unread.clone_from(&u.unread);
        }
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
    print_summary(&report);
    // One package's run answers a question: it is not the day's report.
    if opts.name.is_none() {
        let dir = repo.join("build/audit");
        fs::create_dir_all(&dir)?;
        let day = report.generated.get(..10).unwrap_or("today").to_string();
        let v = serde_json::to_value(&report)?;
        crate::util::write_pretty(&dir.join(format!("{day}.json")), &v)?;
        crate::util::write_pretty(&dir.join("latest.json"), &v)?;
        println!("report: {}", dir.join(format!("{day}.json")).display());
    }
    // A malicious-package record is not a thing to triage (R57).
    let bad: Vec<String> = report
        .packages
        .iter()
        .flat_map(|p| p.hits.iter().map(move |h| (p, h)))
        .filter(|(_, h)| h.id.starts_with("MAL-") || h.aliases.iter().any(|a| a.starts_with("MAL-")))
        .map(|(p, h)| format!("pkg:{} {}: {}", p.name, p.version, h.id))
        .collect();
    if !bad.is_empty() {
        bail!("MALICIOUS packages, by OSV's records:\n  {}", bad.join("\n  "));
    }
    Ok(())
}

/// `latest.json` to the default origin (R61), signed by this build host's
/// key as a release is, so a machine can check it. Its own key, not part of
/// a release: findings move daily and a release does not.
fn publish(repo: &Path) -> Result<()> {
    let file = repo.join("build/audit/latest.json");
    let bytes = fs::read(&file)
        .with_context(|| format!("{}: run `cargo make audit` first", file.display()))?;
    let report: Value = serde_json::from_slice(&bytes)?;
    if report["packages"].as_array().is_none_or(|p| p.is_empty()) {
        bail!("{} lists no packages", file.display());
    }
    let sig = PathBuf::from(format!("{}.sig", file.display()));
    let _ = fs::remove_file(&sig);
    crate::util::run_out(
        Command::new("ssh-keygen")
            .args(["-Y", "sign", "-q", "-n", oath_core::update::AUDIT_NAMESPACE, "-f"])
            .arg(crate::release::sign_key())
            .arg(&file),
    )
    .context("ssh-keygen -Y sign")?;
    let who = oath_core::sshsig::verify(
        &bytes,
        &fs::read_to_string(&sig)?,
        oath_core::update::AUDIT_NAMESPACE,
        &[crate::release::public_key()?],
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let origin = crate::origin::Origin::from_env()?;
    let key = oath_core::update::AUDIT_KEY;
    origin.put(&format!("{key}.sig"), &sig)?;
    origin.put(key, &file)?;
    println!(
        "{key} is on the origin: {} packages, generated {}, signed by {who}",
        report["packages"].as_array().map_or(0, Vec::len),
        report["generated"].as_str().unwrap_or("?")
    );
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
    for p in &r.packages {
        let version = p.upstream.as_deref().unwrap_or(&p.version);
        print_upstream(&p.name, version, p.newer.as_deref(), p.upstream_unread.as_deref());
    }
}

fn print_upstream(name: &str, version: &str, newer: Option<&str>, unread: Option<&str>) {
    if let Some(n) = newer {
        println!("pkg:{name} {version}: upstream has {n}");
    }
    if let Some(w) = unread {
        println!("pkg:{name} {version}: upstream unread: {w}");
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
/// The upstream's version for a pin (`version_after` applied).
pub fn pin_version(entry: &Entry, version: &str) -> String {
    match &entry.version_after {
        Some(m) => version.split(m.as_str()).nth(1).unwrap_or(version).to_string(),
        None => version.to_string(),
    }
}

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
        let version = pin_version(&entry, &p.version);
        let product = lock.get(name).map(|e| e.product.clone()).unwrap_or_default();
        let purls = ecosystem_purls(&cache.join("pkg").join(name).join(&product));
        out.push(Target {
            name: name.clone(),
            version,
            pin: p.version.clone(),
            hash: product,
            entry,
            purls,
            patched: patched(&crate::pkgs::pkgs_dir(repo).join(name).join("patches")),
            listing: p.source.first().and_then(|s| listing_of(&s.url, &p.version)).map(
                |(dir, file)| (entries[name].page.clone().unwrap_or(dir), file),
            ),
        });
    }
    // pkg:oath, which cargo builds from this tree: its crates, and what
    // `[oath]` says of them.
    if only.as_deref().is_none_or(|o| o == "oath") {
        let product = fs::read_to_string(repo.join("build/pkgs/oath.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["product"].as_str().map(str::to_string))
            .unwrap_or_default();
        let tree = cache.join("pkg/oath").join(&product);
        let version = own_version(&tree, "oath").unwrap_or_else(|| "this tree".into());
        let mut entry = entries.get("oath").cloned().unwrap_or_default();
        entry.skip.get_or_insert_with(|| "Oath's own: its crates go to OSV".into());
        out.push(Target {
            name: "oath".into(),
            pin: version.clone(),
            version,
            hash: product,
            entry,
            purls: ecosystem_purls(&tree),
            patched: BTreeMap::new(),
            listing: None,
        });
    }
    // A pack a profile pins from elsewhere (R28): its owner builds it, so
    // only what its `components.json` names can be matched.
    for profile in oath_core::profiles() {
        for (name, p) in crate::pinned::of_profile(repo, &profile)? {
            if only.as_ref().is_some_and(|o| *o != name) || out.iter().any(|t| t.name == name) {
                continue;
            }
            let tree = cache.join("pkg").join(&name).join(&p.hash);
            let version = own_version(&tree, &name).unwrap_or_default();
            out.push(Target {
                name,
                pin: version.clone(),
                version,
                hash: p.hash,
                entry: Entry {
                    skip: Some(
                        "built by its owner: only the ecosystem packages its components.json \
                         names go to OSV"
                            .into(),
                    ),
                    ..Default::default()
                },
                purls: ecosystem_purls(&tree),
                patched: BTreeMap::new(),
                listing: None,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn own_version(tree: &Path, name: &str) -> Option<String> {
    oath_core::own_version(&oath_core::read_components(tree)?, name)
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

/// CVE or GitHub advisory id: patch file, for each `*CVE-YYYY-N*.patch` or
/// `*GHSA-xxxx-xxxx-xxxx*.patch` in `dir`.
fn patched(dir: &Path) -> BTreeMap<String, String> {
    let re = regex::Regex::new(r"CVE-\d{4}-\d{4,}|GHSA(?:-[0-9a-z]{4}){3}").expect("regex");
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

/// A version without its build (Temurin's `21.0.13+9`): records name the
/// release.
fn release_of(v: &str) -> &str {
    v.split('+').next().unwrap_or(v)
}

/// The CPE NVD matches for this version: OpenSSH's `10.6p1` is version
/// `10.6`, update `p1`.
fn cpe_with_version(cpe: &str, version: &str) -> String {
    let version = release_of(version);
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
/// release, `p1` after it, `*` above anything. A build (Temurin's `+9`)
/// orders only two builds of one release, so `21.0.13.1+2` is past
/// `21.0.13+9`.
pub fn cmp_versions(a: &str, b: &str) -> Ordering {
    let order = cmp_toks(&toks(release_of(a)), &toks(release_of(b)));
    match (a.split_once('+'), b.split_once('+')) {
        (Some((_, x)), Some((_, y))) if order == Ordering::Equal => cmp_toks(&toks(x), &toks(y)),
        _ => order,
    }
}

fn cmp_toks(a: &[Tok], b: &[Tok]) -> Ordering {
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

/// What an entry whose listed versions mean "this and earlier" says of
/// `version` (`listed_and_earlier`): affected at or below the version listed
/// for its line (its first number). A listed version may carry its product
/// (`Oracle Java SE:21.0.5`), and another product's are not this one's.
pub fn status_through(entry: &Value, version: &str) -> Says {
    let product = norm(entry["product"].as_str().unwrap_or(""));
    let line = |v: &str| toks(v).into_iter().next();
    let mine = line(version);
    for v in entry["versions"].as_array().into_iter().flatten() {
        if v["status"].as_str() != Some("affected") {
            continue;
        }
        let raw = v["version"].as_str().unwrap_or("");
        let listed = match raw.rsplit_once(':') {
            Some((p, x)) if norm(p) == product => x,
            Some(_) => continue,
            None => raw,
        };
        if line(listed) == mine && cmp_versions(version, listed) != Ordering::Greater {
            return Says::Range("affected");
        }
    }
    Says::Default("unaffected")
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

/// What the scan keeps between runs: the CVE List's mirror, GitHub's reads.
fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/home".into());
    home.join(".local/share/oath")
}

fn cvelist_dir() -> PathBuf {
    std::env::var_os("OATH_CVELIST").map(PathBuf::from).unwrap_or_else(|| data_dir().join("cvelist"))
}

fn norm(s: &str) -> String {
    s.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

const NO_VENDOR: &[&str] = &["", "na", "unknown", "unspecified", "notavailable"];

/// A vendor as its CPE names it: "Oracle Corporation" is `oracle`.
fn bare_vendor(v: &str) -> &str {
    ["corporation", "inc", "llc", "ltd", "limited", "gmbh"]
        .iter()
        .find_map(|s| v.strip_suffix(s).filter(|b| !b.is_empty()))
        .unwrap_or(v)
}

/// Product names too common to trust without the vendor.
const GENERIC: &[&str] = &[
    "bc", "make", "less", "tar", "sed", "grep", "flex", "patch", "m4", "gzip", "git", "glib",
    "mesa", "node", "rust", "python", "perl", "steam", "linux", "cli", "codex", "chrome", "aws",
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
        // With no CPE there is no vendor to hold a record to: its name decides.
        let unheld = NO_VENDOR.contains(&vendor.as_str()) || self.vendors.is_empty();
        self.vendors.contains(&vendor)
            || self.vendors.contains(bare_vendor(&vendor))
            || self.products.contains(&vendor)
            || unheld && !GENERIC.contains(&product.as_str())
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
                let version = release_of(&t.version);
                let says_of = |a: &Value| {
                    if t.entry.listed_and_earlier {
                        status_through(a, version)
                    } else {
                        status_of(a, version)
                    }
                };
                let judge = |list: &[Value]| -> Vec<Says> {
                    list.iter().filter(|a| m.matches(a)).map(says_of).collect()
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

// ---------- GitHub's repository advisories ----------

/// A repository's published advisories, as last read.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Advisories {
    read: String,
    advisories: Vec<Value>,
}

/// `owner/name` of a GitHub repository's URL.
fn github_repo(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://github.com/")?.trim_end_matches('/');
    let mut parts = rest.trim_end_matches(".git").split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// A project's advisory on GitHub reaches no CVE record until one is
/// assigned, days or weeks later (vim's GHSA-p99j, 2026-11-08), so the scan
/// reads each GitHub repository's own: `repo`, or `compare_repo`. GitHub
/// answers 60 unkeyed requests an hour, so a run reads the `GHSA_READS` read
/// longest ago (`--offline`: none) and keeps every repository's last read in
/// `ghsa.json`; each is matched as last read, at most a run or two old.
/// Returns the source's status and its hits.
fn ghsa(targets: &[Target], offline: bool) -> (String, Vec<(String, Hit)>) {
    let mut repos: BTreeMap<String, Vec<&Target>> = BTreeMap::new();
    for t in targets.iter().filter(|t| t.entry.skip.is_none()) {
        let url = t.entry.repo.as_deref().or(t.entry.compare_repo.as_deref());
        if let Some(r) = url.and_then(github_repo) {
            repos.entry(r).or_default().push(t);
        }
    }
    let file = data_dir().join("ghsa.json");
    let mut cache: BTreeMap<String, Advisories> = fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut due: Vec<&String> = repos.keys().collect();
    due.sort_by_key(|r| cache.get(*r).map(|a| a.read.clone()).unwrap_or_default());
    let (mut read, mut failed) = (0, Vec::new());
    let mut left = if offline { 0 } else { GHSA_READS };
    for r in due {
        match advisories_of(r, &mut left) {
            Ok(Some(list)) => {
                read += 1;
                let now = oath_core::now_rfc3339();
                cache.insert(r.clone(), Advisories { read: now, advisories: list });
            }
            Ok(None) => break,
            Err(e) => {
                let e = format!("{e:#}");
                // Out of requests for the hour: the rest wait for the next run.
                let limited = e.contains("error: 403") || e.contains("error: 429");
                failed.push(if limited { format!("{r}: rate limited") } else { format!("{r}: {e}") });
                if limited {
                    break;
                }
            }
        }
    }
    if read > 0 {
        if let Err(e) = crate::util::write_pretty(&file, &cache) {
            failed.push(format!("{}: {e:#}", file.display()));
        }
    }
    let mut out = Vec::new();
    for (r, ts) in &repos {
        let Some(c) = cache.get(r) else { continue };
        for a in c.advisories.iter().filter(|a| a["withdrawn_at"].is_null()) {
            for t in ts {
                let tag = t.entry.tag.as_deref();
                if let Some(s) = ghsa_status(a, release_of(&t.version), tag) {
                    out.push((t.name.clone(), ghsa_hit(a, s)));
                }
            }
        }
    }
    let unread = repos.keys().filter(|r| !cache.contains_key(*r)).count();
    let oldest = repos.keys().filter_map(|r| cache.get(r)).map(|a| a.read.clone()).min();
    let mut status = format!("{} repositories, {read} read now", repos.len());
    if let Some(o) = oldest.filter(|_| read + unread < repos.len()) {
        status += &format!(", the rest as read since {}", o.get(..10).unwrap_or(&o));
    }
    if unread > 0 {
        status += &format!(", {unread} never read");
    }
    if !failed.is_empty() {
        status += &format!("; failed: {}", failed.join("; "));
    }
    let none = !offline && read == 0 && !repos.is_empty();
    (format!("{}{status}", if none { "error: " } else { "ok, " }), out)
}

/// One repository's published advisories, trimmed to what the scan reads;
/// `None` when this run's requests ran out first.
fn advisories_of(repo: &str, left: &mut usize) -> Result<Option<Vec<Value>>> {
    let mut out = Vec::new();
    for page in 1.. {
        if *left == 0 {
            return Ok(None);
        }
        *left -= 1;
        let url = format!(
            "{GITHUB}/repos/{repo}/security-advisories?state=published&per_page=100&page={page}"
        );
        let raw = http(&url, None, Some("Accept: application/vnd.github+json"))?;
        let list: Vec<Value> = serde_json::from_str(&raw).context("advisories JSON")?;
        let full = list.len() == 100;
        out.extend(list.into_iter().map(|a| {
            let vulns: Vec<Value> = a["vulnerabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| {
                    json!({
                        "vulnerable_version_range": v["vulnerable_version_range"],
                        "patched_versions": v["patched_versions"],
                    })
                })
                .collect();
            json!({
                "ghsa_id": a["ghsa_id"], "cve_id": a["cve_id"], "severity": a["severity"],
                "summary": a["summary"], "html_url": a["html_url"],
                "withdrawn_at": a["withdrawn_at"], "vulnerabilities": vulns,
            })
        }));
        if !full {
            break;
        }
    }
    Ok(Some(out))
}

/// A version as an advisory writes it, as the pin's: `v9.2.1352`, a tag
/// (`mbedtls-3.6.8`), or the pin's own form (llama.cpp's `b7437`); not a
/// commit, and not words.
fn bare_version(v: &str, tag: Option<&str>, pin: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() || v.contains(char::is_whitespace) || commit_like(v) {
        return None;
    }
    if v.trim_start_matches(['v', 'V']).starts_with(|c: char| c.is_ascii_digit()) {
        return Some(v.to_string());
    }
    if let Some(t) = tag.and_then(|t| tag_versions(t, &[v.to_string()]).into_iter().next()) {
        return Some(t);
    }
    let lead = |s: &str| toks(s).into_iter().next();
    (matches!(lead(v), Some(Tok::Word(_))) && lead(v) == lead(pin)).then(|| v.to_string())
}

const OPS: [&str; 5] = ["<=", ">=", "<", ">", "="];

/// One part of a range: `< v9.2.1352`, `>=3.0`, or a span bounded on both
/// sides (`0.2.0 <= 0.38.0`, crun's `0.12.2 <= VERSION <= 1.29.1`).
fn constraints(part: &str) -> Option<Vec<(&'static str, &str)>> {
    if let Some(c) = OPS.iter().find_map(|o| part.strip_prefix(o).map(|v| (*o, v.trim()))) {
        return Some(vec![c]);
    }
    let op = |s: &str| OPS.iter().find(|o| **o == s).copied();
    let flip = |s: &str| match s {
        "<=" => Some(">="),
        "<" => Some(">"),
        _ => None,
    };
    match part.split_whitespace().collect::<Vec<_>>().as_slice() {
        [lo, o, hi] => Some(vec![(flip(o)?, *lo), (op(o)?, *hi)]),
        [lo, o1, _, o2, hi] => Some(vec![(flip(o1)?, *lo), (op(o2)?, *hi)]),
        _ => None,
    }
}

/// Whether GitHub's range (`>= 9.0, < v9.2.1352`, or with `&&`) holds
/// `version`; `None` when any part of it cannot be read (git's lists of
/// each line's newest affected release).
fn in_range(version: &str, range: &str, tag: Option<&str>) -> Option<bool> {
    let parts: Vec<&str> = range
        .split(',')
        .flat_map(|p| p.split("&&"))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut holds = true;
    for part in &parts {
        let cs = match constraints(part) {
            Some(c) => c,
            // One version alone is that version (GitHub's `= 1.0`).
            None if parts.len() == 1 => vec![("=", *part)],
            None => return None,
        };
        for (op, v) in cs {
            let o = cmp_versions(version, &bare_version(v, tag, version)?);
            holds &= match op {
                "<=" => o != Ordering::Greater,
                ">=" => o != Ordering::Less,
                "<" => o == Ordering::Less,
                ">" => o == Ordering::Greater,
                _ => o == Ordering::Equal,
            };
        }
    }
    Some(holds)
}

/// Whether `version` is below its line's fixed version in an advisory's
/// list (`3.5.0`, `>= 3.4.3`, git's `2.50.2, 2.51.3 … and 2.56.1` or
/// `v2.45.1 v2.44.1`): past every one is fixed, and between lines is not
/// read.
fn below_fix(version: &str, patched: &str, tag: Option<&str>) -> Option<bool> {
    let fixes: Vec<String> = patched
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|p| p.trim_start_matches(['>', '=']))
        .filter(|p| !p.is_empty() && *p != "and")
        .map(|p| bare_version(p, tag, version))
        .collect::<Option<_>>()?;
    let line = |v: &str| toks(v).into_iter().take(2).collect::<Vec<_>>();
    let fix = match fixes.as_slice() {
        [] => return None,
        [one] => one,
        many => match many.iter().find(|f| line(f) == line(version)) {
            Some(f) => f,
            None => {
                let past = many.iter().all(|f| cmp_versions(version, f) == Ordering::Greater);
                return past.then_some(false);
            }
        },
    };
    Some(cmp_versions(version, fix) == Ordering::Less)
}

/// What a repository advisory says of `version`: `affected` when its range
/// holds it and it is below the fixed version, `unknown` when neither can
/// be read (a commit), and nothing when what can be read leaves it out. A
/// fixed version bounds a range left open (vim's `>v9.1.1231`, fixed in
/// v9.1.1406).
fn ghsa_status(a: &Value, version: &str, tag: Option<&str>) -> Option<&'static str> {
    let vulns = a["vulnerabilities"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut unread = vulns.is_empty();
    for v in vulns {
        let range = v["vulnerable_version_range"].as_str().unwrap_or("");
        let fixed = v["patched_versions"].as_str().unwrap_or("");
        let held = match (in_range(version, range, tag), below_fix(version, fixed, tag)) {
            (Some(r), Some(b)) => Some(r && b),
            (r, b) => r.or(b),
        };
        match held {
            Some(true) => return Some("affected"),
            Some(false) => {}
            None => unread = true,
        }
    }
    unread.then_some("unknown")
}

fn ghsa_hit(a: &Value, status: &str) -> Hit {
    let fixed = a["vulnerabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["patched_versions"].as_str())
        .flat_map(|p| p.split(','))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    Hit {
        id: a["ghsa_id"].as_str().unwrap_or("").to_string(),
        aliases: a["cve_id"].as_str().map(str::to_string).into_iter().collect(),
        sources: BTreeSet::from(["GitHub".to_string()]),
        status: status.into(),
        severity: a["severity"].as_str().unwrap_or("").to_ascii_uppercase(),
        summary: short(a["summary"].as_str().unwrap_or("")),
        fixed,
        url: a["html_url"].as_str().unwrap_or("").to_string(),
        ..Default::default()
    }
}

// ---------- upstream's releases ----------

/// A package against its upstream's newest stable release.
#[derive(Clone, Debug, Default)]
struct Upstream {
    newer: Option<String>,
    unread: Option<String>,
}

/// Each package's upstream: a record can come days after the release that
/// fixes it (QEMU 11.1.3 named nine CVEs, eight with no public record), so
/// the audit also asks what upstream has released. A package with `skip` or
/// `unchecked` is left out.
fn upstream(targets: &[Target]) -> BTreeMap<String, Upstream> {
    let mut out = BTreeMap::new();
    for t in targets {
        if t.entry.skip.is_some() || t.entry.unchecked.is_some() {
            continue;
        }
        let u = match upstream_versions(t) {
            Ok(v) => {
                let newer = newer_than(&t.version, &v, &t.entry);
                // Nothing newer, and no sign of the pin: not a read to trust.
                let unread = (newer.is_none() && !v.contains(&t.version))
                    .then(|| format!("upstream's releases do not name {}", t.version));
                Upstream { newer, unread }
            }
            Err(e) => Upstream { newer: None, unread: Some(format!("{e:#}")) },
        };
        out.insert(t.name.clone(), u);
    }
    out
}

fn upstream_status<'a>(all: impl Iterator<Item = &'a Upstream>) -> String {
    let (mut read, mut behind, mut unread) = (0, 0, 0);
    for u in all {
        if u.unread.is_some() {
            unread += 1;
        } else {
            read += 1;
        }
        behind += u.newer.is_some() as usize;
    }
    format!("ok, {read} read, {behind} behind, {unread} unread")
}

fn upstream_versions(t: &Target) -> Result<Vec<String>> {
    let versions = match (&t.entry.repo, &t.entry.tag, &t.listing) {
        (Some(repo), Some(tag), _) => tag_versions(tag, &ls_tags(repo)?),
        (_, _, Some((dir, file))) => listing_versions(file, &http(dir, None, None)?),
        _ => bail!("no repo and tag, and its source's directory names the version"),
    };
    if versions.is_empty() {
        bail!("no release matches");
    }
    Ok(versions)
}

/// The directory a source comes from and its name there with `{version}`,
/// unless the directory itself names the version.
fn listing_of(url: &str, version: &str) -> Option<(String, String)> {
    let (dir, file) = url.rsplit_once('/')?;
    if dir.contains(version) || !file.contains(version) {
        return None;
    }
    Some((format!("{dir}/"), file.replace(version, "{version}")))
}

fn ls_tags(repo: &str) -> Result<Vec<String>> {
    let out = crate::util::run_out(
        Command::new("git")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=30"])
            .args(["ls-remote", "--tags", "--refs", repo]),
    )?;
    Ok(out
        .lines()
        .filter_map(|l| l.split_once("refs/tags/").map(|(_, t)| t.to_string()))
        .collect())
}

/// `pattern` as a regex source around a captured version, and whether the
/// version writes its dots as `_`.
fn version_regex(pattern: &str) -> (String, bool) {
    let underscored = pattern.contains("{version_}");
    let one = pattern.replace("{version_}", "{version}");
    let (before, after) = one.split_once("{version}").unwrap_or((&one, ""));
    let v = r"([0-9][0-9A-Za-z.+~_-]*?)";
    (format!("{}{v}{}", regex::escape(before), regex::escape(after)), underscored)
}

fn tag_versions(pattern: &str, tags: &[String]) -> Vec<String> {
    let (src, under) = version_regex(pattern);
    let re = regex::Regex::new(&format!("^{src}$")).expect("regex");
    tags.iter()
        .filter_map(|t| re.captures(t))
        .map(|c| if under { c[1].replace('_', ".") } else { c[1].to_string() })
        .collect()
}

/// The versions of `pattern`'s files a directory page names.
fn listing_versions(pattern: &str, page: &str) -> Vec<String> {
    let (src, _) = version_regex(pattern);
    let edge = r"[^A-Za-z0-9._-]";
    let re = regex::Regex::new(&format!("(?m)(?:^|{edge}){src}(?:$|{edge})")).expect("regex");
    re.captures_iter(page).map(|c| c[1].to_string()).collect()
}

/// A release, by its version alone: no `rc`, `beta` or other word but a
/// `p1` after it.
fn is_stable(v: &str, e: &Entry) -> bool {
    let toks = toks(v);
    let mut nums = toks.iter().filter_map(|t| match t {
        Tok::Num(n) => Some(*n),
        _ => None,
    });
    let (minor, micro) = (nums.nth(1), nums.next());
    matches!(toks.first(), Some(Tok::Num(_)))
        && toks.iter().all(|t| !matches!(t, Tok::Word(w) if !post(w)))
        && !(e.dev_odd && minor.is_some_and(|m| m % 2 == 1))
        && !e.dev_micro.is_some_and(|d| micro.is_some_and(|m| m >= u128::from(d)))
        && e.series.as_ref().is_none_or(|s| v == s || v.starts_with(&format!("{s}.")))
}

/// The newest stable version in `versions`, when it is past `pin`.
fn newer_than(pin: &str, versions: &[String], e: &Entry) -> Option<String> {
    versions
        .iter()
        .filter(|v| is_stable(v, e))
        .max_by(|a, b| cmp_versions(a, b))
        .filter(|v| cmp_versions(v, pin) == Ordering::Greater)
        .cloned()
}

// ---------- NVD ----------

/// NVD's status (each CPE that failed named) and its hits.
fn nvd(targets: &[Target]) -> Result<(String, Vec<(String, Hit)>)> {
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
    if n > 0 && errors.len() == n {
        bail!("{n} CPEs failed: {}", errors.join("; "));
    }
    let status = match errors.len() {
        0 => format!("ok, {n} CPEs"),
        e => format!("ok, {} of {n} CPEs; failed: {}", n - e, errors.join("; ")),
    };
    Ok((status, out))
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
/// key never reaches argv). `--compressed`: Valve's archive sends its
/// listing gzipped whatever the request asks.
pub(crate) fn http(url: &str, body: Option<&str>, header: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-f", "-L", "--compressed", "--retry", "2", "--max-time", "180", "-K", "-"]);
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
        assert_eq!(cmp_versions("21.0.13.1+2", "21.0.13+9"), Greater);
        assert_eq!(cmp_versions("21.0.13+9", "21.0.13+10"), Less);
        assert_eq!(cmp_versions("21.0.13", "21.0.13+9"), Equal);
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
    fn oracle_lists_the_newest_affected_update_of_each_line() {
        use Says::*;
        // CVE-2026-114938: Temurin's 21.0.13 is at or below Oracle's 21.0.13.0.3.
        let now = json!({ "vendor": "Oracle Corporation", "product": "Oracle Java SE", "versions": [
            { "version": "8u511b33-perf", "status": "affected" },
            { "version": "21.0.13.0.3", "status": "affected" },
            { "version": "25.0.5.0.3", "status": "affected" } ] });
        assert_eq!(status_through(&now, "21.0.13"), Range("affected"));
        assert_eq!(status_through(&now, "21.0.13.1"), Default("unaffected"));
        assert_eq!(status_through(&now, "17.0.20"), Default("unaffected"));
        // Older records name the product in each version, GraalVM's too.
        let then = json!({ "product": "Oracle Java SE", "versions": [
            { "version": "Oracle Java SE:21.0.5", "status": "affected" },
            { "version": "Oracle GraalVM for JDK:21.0.13", "status": "affected" } ] });
        assert_eq!(status_through(&then, "21.0.4"), Range("affected"));
        assert_eq!(status_through(&then, "21.0.13"), Default("unaffected"));
        assert_eq!(release_of("21.0.13+9"), "21.0.13");
        let jdk = Matcher::of(&Target {
            name: "jdk-21".into(),
            entry: Entry {
                cpe: Some("cpe:2.3:a:oracle:openjdk".into()),
                names: vec!["openjdk".into(), "Oracle Java SE".into()],
                ..Entry::default()
            },
            ..Target::default()
        });
        assert!(jdk.matches(&now));
        assert!(!jdk.matches(&json!({ "vendor": "Oracle Corporation", "product": "Oracle GraalVM for JDK" })));
        assert!(!jdk.matches(&json!({ "vendor": "Someone Inc.", "product": "Oracle Java SE" })));
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
        // With no CPE there is no vendor to hold it to: a specific name is enough.
        let named = Matcher::of(&Target { name: "libdisplay-info".into(), ..Default::default() });
        assert!(named.matches(&json!({ "vendor": "emersion", "product": "libdisplay-info" })));
        assert!(!named.matches(&json!({ "vendor": "emersion", "product": "other" })));
        let git = Matcher::of(&Target { name: "git".into(), ..Default::default() });
        assert!(!git.matches(&json!({ "vendor": "someone", "product": "git" })));
    }

    #[test]
    fn tags_cpes_and_patches() {
        assert_eq!(tag_of("curl-{version_}", "8.23.0"), "curl-8_23_0");
        assert_eq!(tag_of("v{version}", "1.3.2"), "v1.3.2");
        assert_eq!(cpe_with_version("cpe:2.3:a:openbsd:openssh", "10.6p1"), "cpe:2.3:a:openbsd:openssh:10.6:p1");
        assert_eq!(cpe_with_version("cpe:2.3:a:haxx:curl", "8.23.0"), "cpe:2.3:a:haxx:curl:8.23.0");
        assert_eq!(cpe_with_version("cpe:2.3:a:oracle:openjdk", "21.0.13+9"), "cpe:2.3:a:oracle:openjdk:21.0.13");
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("oath-backport-CVE-2026-12345.patch"), "").unwrap();
        fs::write(d.path().join("upstream-CVE-2026-99999-fix.patch"), "").unwrap();
        fs::write(d.path().join("upstream-GHSA-p99j-mfjp-r4cg.patch"), "").unwrap();
        let p = patched(d.path());
        assert_eq!(p["CVE-2026-12345"], "oath-backport-CVE-2026-12345.patch");
        assert!(p.contains_key("CVE-2026-99999"));
        assert_eq!(p["GHSA-p99j-mfjp-r4cg"], "upstream-GHSA-p99j-mfjp-r4cg.patch");
    }

    #[test]
    fn a_repository_advisory_says_whether_a_version_is_affected() {
        assert_eq!(github_repo("https://github.com/vim/vim.git").as_deref(), Some("vim/vim"));
        assert_eq!(github_repo("https://github.com/Mbed-TLS/mbedtls/").as_deref(), Some("Mbed-TLS/mbedtls"));
        assert_eq!(github_repo("https://gitlab.com/x/y"), None);
        let adv = |range: &str, fixed: &str| {
            json!({ "ghsa_id": "GHSA-j6wv-2276-36jq", "cve_id": null, "severity": "low",
                "vulnerabilities": [{ "vulnerable_version_range": range, "patched_versions": fixed }] })
        };
        let vim = adv("< v9.2.1352", "v9.2.1352");
        assert_eq!(ghsa_status(&vim, "9.2.1250", Some("v{version}")), Some("affected"));
        assert_eq!(ghsa_status(&vim, "9.2.1352", Some("v{version}")), None);
        let both = adv(">= 3.0.0, <= mbedtls-3.6.7", "");
        assert_eq!(ghsa_status(&both, "3.6.7", Some("mbedtls-{version}")), Some("affected"));
        assert_eq!(ghsa_status(&both, "3.6.8", Some("mbedtls-{version}")), None);
        assert_eq!(ghsa_status(&both, "2.28.9", Some("mbedtls-{version}")), None);
        // `&&`, and an open range the fixed version bounds (vim's).
        let and = adv("> v9.1.1390 && < v9.2.0172", "v9.2.0172");
        assert_eq!(ghsa_status(&and, "9.2.0100", Some("v{version}")), Some("affected"));
        assert_eq!(ghsa_status(&and, "9.2.1250", Some("v{version}")), None);
        assert_eq!(ghsa_status(&adv(">v9.1.1231", "v9.1.1406"), "9.2.1250", Some("v{version}")), None);
        assert_eq!(ghsa_status(&adv(">v9.1.1231", "v9.1.1406"), "9.1.1300", Some("v{version}")), Some("affected"));
        // git lists each line's newest affected release, and its fixes.
        let git = adv("<= 2.50.1, 2.51.0-2.51.2, 2.52.0, 2.56.0", "2.50.2, 2.51.3, 2.52.1 and 2.56.1");
        assert_eq!(ghsa_status(&git, "2.56.0", Some("v{version}")), Some("affected"));
        assert_eq!(ghsa_status(&git, "2.56.1", Some("v{version}")), None);
        assert_eq!(ghsa_status(&git, "2.57.0", Some("v{version}")), None);
        assert_eq!(ghsa_status(&git, "2.53.0", Some("v{version}")), Some("unknown"));
        let spaced = adv("v2.45.0 v2.44.0 <=v2.43.3", ">= v2.44.1, v2.45.1, and v2.43.4");
        assert_eq!(ghsa_status(&spaced, "2.45.0", Some("v{version}")), Some("affected"));
        assert_eq!(ghsa_status(&spaced, "2.56.1", Some("v{version}")), None);
        assert_eq!(ghsa_status(&adv("<=v2.48.1 <=v2.47.1", "(Git for Windows v2.47.1(2))"), "2.56.1", None), Some("unknown"));
        // Spans, `>=` before a fix, a pin's own form, and commits.
        let crun = adv("0.12.2 <= VERSION <= 1.29.1", "");
        assert_eq!(ghsa_status(&crun, "1.29.0", Some("{version}")), Some("affected"));
        assert_eq!(ghsa_status(&crun, "1.31", Some("{version}")), None);
        assert_eq!(ghsa_status(&adv("0.2.0 <= 0.38.0", "0.39.0"), "0.169.0", None), None);
        assert_eq!(ghsa_status(&adv("< 3.4.3", ">= 3.4.3"), "3.4.2", None), Some("affected"));
        assert_eq!(ghsa_status(&adv("< b7437", "b7824"), "b11960", Some("{version}")), None);
        assert_eq!(ghsa_status(&adv("< b7437", "b7824"), "b7000", Some("{version}")), Some("affected"));
        assert_eq!(ghsa_status(&adv("<26a48ad", "26a48ad"), "b11960", Some("{version}")), Some("unknown"));
        // No range: below the fixed version; neither: unknown.
        assert_eq!(ghsa_status(&adv("", "1.2.0"), "1.1.9", None), Some("affected"));
        assert_eq!(ghsa_status(&adv("", "1.2.0"), "1.2.0", None), None);
        assert_eq!(ghsa_status(&adv("", ""), "1.0", None), Some("unknown"));
        assert_eq!(ghsa_status(&json!({ "vulnerabilities": [] }), "1.0", None), Some("unknown"));
        let h = ghsa_hit(&vim, "affected");
        assert_eq!((h.id.as_str(), h.severity.as_str(), h.fixed.clone()), ("GHSA-j6wv-2276-36jq", "LOW", vec!["v9.2.1352".to_string()]));
    }

    #[test]
    fn upstream_releases_past_the_pin() {
        let e = Entry::default();
        let vs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(newer_than("11.1.2", &vs(&["11.1.2", "11.1.3", "11.2.0-rc1"]), &e).as_deref(), Some("11.1.3"));
        assert_eq!(newer_than("1.3.2", &vs(&["1.3.1", "1.3.2"]), &e), None);
        assert_eq!(newer_than("2.6.4", &vs(&["2.6.4", "2.6.5-rc1"]), &e), None);
        assert_eq!(newer_than("10.6p1", &vs(&["10.6p1", "10.6p2"]), &e).as_deref(), Some("10.6p2"));
        let series = Entry { series: Some("7.3".into()), ..Default::default() };
        assert_eq!(newer_than("7.3.1", &vs(&["7.3.1", "7.3.2", "7.4", "7.30.1"]), &series).as_deref(), Some("7.3.2"));
        let odd = Entry { dev_odd: true, ..Default::default() };
        assert_eq!(newer_than("5.44.0", &vs(&["5.44.0", "5.45.4"]), &odd), None);
        let micro = Entry { dev_micro: Some(90), ..Default::default() };
        assert_eq!(newer_than("1.26.0", &vs(&["1.26.0", "1.26.92", "1.26.1"]), &micro).as_deref(), Some("1.26.1"));

        let tags = vs(&["curl-8_23_0", "curl-8_24_0", "curl-8_24_0-rc1", "tiny-curl-8_4_0", "rel"]);
        assert_eq!(tag_versions("curl-{version_}", &tags), vs(&["8.23.0", "8.24.0", "8.24.0-rc1"]));
        assert_eq!(tag_versions("release-{version}-stable", &vs(&["release-2.1.13-stable", "release-2.2.1-alpha"])), vs(&["2.1.13"]));

        assert_eq!(
            listing_of("https://ftp.gnu.org/gnu/tar/tar-1.35.tar.xz", "1.35"),
            Some(("https://ftp.gnu.org/gnu/tar/".into(), "tar-{version}.tar.xz".into()))
        );
        assert_eq!(listing_of("https://x.org/glib/2.90/glib-2.90.1.tar.xz", "2.90"), None);
        let page = r#"<a href="tar-1.35.tar.xz">tar-1.35.tar.xz</a> <a href="tar-1.35.tar.xz.sig">s</a>
            <a href="tar-1.36.tar.xz">x</a> <a href="star-9.tar.xz">y</a> <a href="tar-latest.tar.xz">z</a>"#;
        let mut got = listing_versions("tar-{version}.tar.xz", page);
        got.dedup();
        assert_eq!(got, vs(&["1.35", "1.36"]));
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

    /// Every pin was compared with its tag (R54), or its entry says why it
    /// has none, and nothing in both differs unless its entry says why.
    #[test]
    fn every_pin_with_a_tag_was_compared() {
        let repo = crate::util::repo_root();
        let pkgs = crate::pkgs::load(&repo).unwrap();
        let e = entries(&repo).unwrap();
        for (n, p) in &pkgs {
            let (Some(s), Some(u)) = (p.source.first(), e.get(n)) else { continue };
            if p.i386.is_some() || u.no_compare.is_some() {
                continue;
            }
            assert!(
                crate::tags::tag_of(u, &p.version).is_some(),
                "pkgs/{n}: audit.toml needs compare_repo and compare_tag, or no_compare and why"
            );
            let record = crate::tags::read_record(&repo, n);
            let Some((sha, lines)) = record else {
                panic!("pkgs/{n}: no pkgs/tags/{n}.txt: run cargo make pkgs --fetch {n}");
            };
            assert_eq!(
                sha,
                crate::tags::pin_sha(p, u),
                "pkgs/tags/{n}.txt is for another pin: cargo make pkgs --fetch {n}"
            );
            let _ = s;
            let differs = lines.iter().filter(|l| l.starts_with("differs ")).count();
            assert!(
                differs == 0 || u.differs_because.is_some(),
                "pkgs/{n}: {differs} files differ from the tag: read pkgs/tags/{n}.txt"
            );
        }
    }

    /// Every pin passed intake (R57): recorded, a week old or excused, new
    /// names approved, and whatever moved reviewed.
    #[test]
    fn every_pin_passed_intake() {
        let repo = crate::util::repo_root();
        let pkgs = crate::pkgs::load(&repo).unwrap();
        let e = entries(&repo).unwrap();
        let mut bad = Vec::new();
        for (n, p) in &pkgs {
            let Some(u) = e.get(n).filter(|_| p.i386.is_none() && !p.source.is_empty()) else {
                continue;
            };
            let Some(r) = crate::intake::read(&repo, n) else {
                bad.push(format!("{n}: no pkgs/intake/{n}.txt: cargo make pkgs --fetch {n}"));
                continue;
            };
            if r.pin != crate::intake::all_sha(p) {
                bad.push(format!("{n}: its record is for another pin: cargo make pkgs --fetch {n}"));
            }
            bad.extend(crate::intake::problems(&repo, n, &r, u));
        }
        let lock = fs::read_to_string(repo.join("Cargo.lock")).unwrap();
        match crate::intake::read(&repo, "oath") {
            Some(r) if r.pin == crate::intake::cargo_lock_pin(&lock) => {
                bad.extend(crate::intake::problems(&repo, "oath", &r, &Entry::default()));
            }
            _ => bad.push("Cargo.lock moved: cargo make pkgs --fetch oath".to_string()),
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn every_package_has_an_audit_entry() {
        let repo = crate::util::repo_root();
        let pkgs = crate::pkgs::load(&repo).unwrap();
        let e = entries(&repo).unwrap();
        for (n, p) in &pkgs {
            if p.i386.is_none() {
                assert!(e.contains_key(n), "pkgs/{n} has no entry in pkgs/audit.toml");
                let u = &e[n];
                let tags = u.repo.is_some() && u.tag.is_some();
                let dir = p.source.first().is_some_and(|s| listing_of(&s.url, &p.version).is_some());
                assert!(
                    tags || dir || u.skip.is_some() || u.unchecked.is_some(),
                    "pkgs/{n}: audit.toml needs repo and tag, or unchecked and why"
                );
            }
        }
    }
}
