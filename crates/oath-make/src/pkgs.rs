//! Native packages. `pkgs/<name>/pkg.toml` names a package's upstream
//! sources (pinned by sha256), its licence and the packs it builds with;
//! `pkgs/<name>/build.sh` is the build. `cargo make pkgs` turns each into
//! an `oath build` plan pinned to its source pack and to the exact build
//! packs in `pkgs/lock.json`, writes the plan to `pkgs/<name>/<name>.plan`,
//! and builds it in the sandbox. `cargo make bootstrap` makes the first
//! of each base pack ([`BASE`]: gcc, glibc and the GNU tools) with
//! `pkgs/bootstrap.sh` on the host; after that their plans rebuild them
//! with themselves.
//!
//! A source with `sig` is checked against its upstream's OpenPGP
//! signature, by a key pinned in `pkgs/keys/<key>.asc`, on every fetch; a
//! source without one says why (`unsigned`).
//!
//! Builds run in the catalog at `.cache/oath` (its store is the host store
//! cache the image build reads).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use oath_core::{fmt_plan, PkgNeed, PlanFile};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::util::run;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PkgToml {
    pub version: String,
    pub license: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub home: String,
    /// Package URL for components.json (`pkg:generic/bash@5.3`).
    #[serde(default)]
    pub purl: String,
    #[serde(default)]
    pub source: Vec<Source>,
    /// Packs on PATH in the build sandbox, by name (pinned from the lock).
    #[serde(default)]
    pub build: Vec<String>,
    /// Packs needed at runtime.
    #[serde(default)]
    pub run: Vec<String>,
    /// Why the sources without `sig` have no signature to check.
    #[serde(default)]
    pub unsigned: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub url: String,
    pub sha256: String,
    /// File name in the source pack (default: the URL's last segment).
    #[serde(default)]
    pub file: Option<String>,
    /// URL of the upstream's detached OpenPGP signature over the file (or
    /// over `sums`, or over the file after `decompress`).
    #[serde(default)]
    pub sig: Option<String>,
    /// The signing key: `pkgs/keys/<key>.asc`.
    #[serde(default)]
    pub key: Option<String>,
    /// URL of a `sha256sum` list that `sig` signs and that must name this
    /// file with its `sha256`.
    #[serde(default)]
    pub sums: Option<String>,
    /// `xz`: `sig` signs the decompressed file (kernel.org's `.tar.sign`).
    #[serde(default)]
    pub decompress: Option<String>,
    /// URL of the upstream's Sigstore bundle, for upstreams that sign only
    /// with Sigstore. Checked against `pkgs/keys/sigstore.json`.
    #[serde(default)]
    pub sigstore: Option<String>,
    /// The bundle's signer: the certificate's email or URI.
    #[serde(default)]
    pub identity: Option<String>,
    /// The signer's OIDC issuer (`https://github.com/login/oauth`).
    #[serde(default)]
    pub issuer: Option<String>,
    /// Why there is no signature to check.
    #[serde(default)]
    pub unsigned: Option<String>,
}

impl Source {
    /// Path under `src/` (and under build/fetch); may contain directories.
    fn file(&self) -> String {
        self.file.clone().unwrap_or_else(|| {
            let last = self.url.rsplit('/').next().unwrap_or("source");
            last.split('?').next().unwrap_or(last).to_string()
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LockEntry {
    pub version: String,
    /// SPDX expression from pkg.toml (publish needs it).
    pub license: String,
    /// `plan` (built in the sandbox) or `bootstrap` (pkgs/bootstrap.sh).
    pub by: String,
    /// sha256 of `pkgs/<name>/<name>.plan`; empty for bootstrap.
    #[serde(default)]
    pub plan: String,
    pub product: String,
    /// The packs this product loads at run time, as it was built (its
    /// plan's `run_needs`). Absent in entries from before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Vec<String>>,
}

pub type Lock = BTreeMap<String, LockEntry>;

pub fn pkgs_dir(repo: &Path) -> PathBuf {
    repo.join("pkgs")
}

/// The build catalog: `.cache/oath`, whose `store/` is the host store cache.
pub fn build_catalog(repo: &Path) -> Result<PathBuf> {
    let root = crate::pack::host_store_cache(repo)
        .parent()
        .map(Path::to_path_buf)
        .context("store cache has no parent")?;
    if !root.join("objects").is_dir() {
        oath_core::seed(&root).map_err(|e| anyhow::anyhow!("seed {}: {e}", root.display()))?;
    }
    let cat = oath_core::Catalog::open(&root).map_err(|e| anyhow::anyhow!("{e}"))?;
    let lock = cat.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    cat.migrate(&lock).map_err(|e| anyhow::anyhow!("migrate {}: {e}", root.display()))?;
    Ok(root)
}

/// What `cargo make bootstrap` makes (stage 1), and what every plan
/// builds with besides the packs its `build` names: the compiler, the C
/// library and one GNU tool for each job (R17, R18).
pub const BASE: &[&str] = &[
    "gcc",
    "glibc",
    "bash",
    "coreutils",
    "sed",
    "grep",
    "gawk",
    "findutils",
    "diffutils",
    "patch",
    "tar",
    "gzip",
    "xz",
    "bzip2",
    "make",
];

pub fn load(repo: &Path) -> Result<BTreeMap<String, PkgToml>> {
    let mut out = BTreeMap::new();
    for e in fs::read_dir(pkgs_dir(repo))? {
        let e = e?;
        let toml_p = e.path().join("pkg.toml");
        if !toml_p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let text = fs::read_to_string(&toml_p)?;
        let mut p: PkgToml =
            toml::from_str(&text).with_context(|| format!("parse {}", toml_p.display()))?;
        let extra = std::mem::take(&mut p.build);
        p.build = BASE.iter().map(|b| b.to_string()).collect();
        for d in extra {
            if !p.build.contains(&d) {
                p.build.push(d);
            }
        }
        for s in &p.source {
            check_signed(repo, &p, s)
                .with_context(|| format!("pkgs/{name}/pkg.toml: {}", s.url))?;
        }
        out.insert(name, p);
    }
    Ok(out)
}

/// A source names its OpenPGP signature and key, or its Sigstore bundle
/// and signer, or says why it has none.
fn check_signed(repo: &Path, pkg: &PkgToml, s: &Source) -> Result<()> {
    let pgp = s.key.is_some() || s.sums.is_some() || s.decompress.is_some();
    let store = s.identity.is_some() || s.issuer.is_some();
    match (&s.sig, &s.sigstore) {
        (Some(_), Some(_)) => bail!("has both sig and sigstore"),
        (Some(_), None) => {
            let Some(key) = &s.key else { bail!("sig without key") };
            if s.unsigned.is_some() || store {
                bail!("sig takes key, sums and decompress, not unsigned, identity or issuer");
            }
            let k = key_file(repo, key);
            if !k.is_file() {
                bail!("{} is missing", k.display());
            }
        }
        (None, Some(_)) => {
            if s.identity.is_none() || s.issuer.is_none() {
                bail!("sigstore needs identity and issuer");
            }
            if s.unsigned.is_some() || pgp {
                bail!("sigstore takes identity and issuer, not unsigned, key, sums or decompress");
            }
            let root = sigstore_root(repo);
            if !root.is_file() {
                bail!("{} is missing", root.display());
            }
        }
        (None, None) => {
            if pgp || store {
                bail!("key, sums, decompress, identity and issuer need sig or sigstore");
            }
            if s.unsigned.as_ref().or(pkg.unsigned.as_ref()).is_none_or(|r| r.is_empty()) {
                bail!("no sig or sigstore; say why with unsigned");
            }
        }
    }
    if let Some(d) = s.decompress.as_deref().filter(|d| *d != "xz") {
        bail!("decompress = {d:?}; only xz is supported");
    }
    Ok(())
}

fn key_file(repo: &Path, key: &str) -> PathBuf {
    pkgs_dir(repo).join("keys").join(format!("{key}.asc"))
}

/// Sigstore's trusted root (Fulcio CAs, Rekor keys), as Sigstore publishes it.
fn sigstore_root(repo: &Path) -> PathBuf {
    pkgs_dir(repo).join("keys/sigstore.json")
}

pub fn read_lock(repo: &Path) -> Result<Lock> {
    let p = pkgs_dir(repo).join("lock.json");
    if !p.is_file() {
        return Ok(Lock::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(&p)?)?)
}

fn write_lock(repo: &Path, lock: &Lock) -> Result<()> {
    let mut s = serde_json::to_string_pretty(lock)?;
    s.push('\n');
    fs::write(pkgs_dir(repo).join("lock.json"), s)?;
    Ok(())
}

/// Download (once) and verify every source of `pkg`: its sha256 pin and,
/// when it has one, its upstream signature. Returns each source's path
/// under `src/` and its file in build/fetch.
/// What a package builds with: its build packs and, since they run in the
/// sandbox, what each of those needs to run (bison runs m4), in order:
/// what the locked product needs, not what its pkg.toml says now. The
/// package itself is left out until it has been built once.
pub(crate) fn build_closure(
    repo: &Path,
    all: &BTreeMap<String, PkgToml>,
    lock: &Lock,
    name: &str,
    p: &PkgToml,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut todo: Vec<String> = p.build.clone();
    todo.reverse();
    while let Some(d) = todo.pop() {
        if out.contains(&d) || (d == name && !lock.contains_key(name)) {
            continue;
        }
        todo.extend(locked_run(repo, all, lock, &d).into_iter().rev());
        out.push(d);
    }
    out
}

/// The packs `name`'s locked product runs with: as recorded, or (for an
/// entry from before that) its plan's `run_needs` if that plan is the one
/// the lock pins, or else pkg.toml's. A bootstrap pack carries its own.
pub(crate) fn locked_run(
    repo: &Path,
    all: &BTreeMap<String, PkgToml>,
    lock: &Lock,
    name: &str,
) -> Vec<String> {
    let Some(e) = lock.get(name) else {
        return all.get(name).map(|p| p.run.clone()).unwrap_or_default();
    };
    if let Some(run) = &e.run {
        return run.clone();
    }
    if e.by == "bootstrap" {
        return Vec::new();
    }
    let plan = pkgs_dir(repo).join(name).join(format!("{name}.plan"));
    if let Ok(bytes) = fs::read(&plan) {
        if oath_core::file_hash(&bytes) == e.plan {
            if let Ok(pf) = oath_core::parse_plan(&String::from_utf8_lossy(&bytes)) {
                return pf
                    .run_needs
                    .iter()
                    .map(|n| n.id.trim_start_matches("pkg:").to_string())
                    .collect();
            }
        }
    }
    all.get(name).map(|p| p.run.clone()).unwrap_or_default()
}

/// `cargo make pkgs --fetch`: every source of the named packages (or all),
/// pinned and signature-checked, and nothing built.
pub fn fetch_all(repo: &Path, names: &[String]) -> Result<()> {
    let all = load(repo)?;
    for (name, p) in &all {
        if names.is_empty() || names.contains(name) {
            fetch(repo, p).with_context(|| format!("pkgs/{name}"))?;
            eprintln!("== {name}: {} sources checked", p.source.len());
        }
    }
    Ok(())
}

pub fn fetch(repo: &Path, pkg: &PkgToml) -> Result<Vec<(String, PathBuf)>> {
    let dir = repo.join("build/fetch");
    let mut out = Vec::new();
    for s in &pkg.source {
        check_signed(repo, pkg, s).with_context(|| s.url.clone())?;
        let rel = s.file();
        if rel.split('/').any(|c| c.is_empty() || c == "..") {
            bail!("bad source file name {rel}");
        }
        let dest = dir.join(&rel);
        fs::create_dir_all(dest.parent().context("parent")?)?;
        let fresh = !(dest.is_file() && sha256(&dest)? == s.sha256);
        if fresh {
            eprintln!(">> fetch {}", s.url);
            download(&s.url, &dest, false)?;
        }
        let got = sha256(&dest)?;
        if got != s.sha256 {
            bail!("{}: sha256 {got}, pkg.toml pins {}", dest.display(), s.sha256);
        }
        if s.sig.is_some() {
            let key = format!("pkgs/keys/{}.asc", s.key.as_deref().unwrap_or(""));
            let who = verify_sig(repo, s, &dest)
                .with_context(|| format!("signature of {} (checked against {key})", s.url))?;
            if fresh {
                eprintln!(">> signed by {who} ({key})");
            }
        } else if let (Some(url), Some(who), Some(via)) = (&s.sigstore, &s.identity, &s.issuer) {
            let entry = crate::sigstore::verify(
                &fs::read(sigstore_root(repo))?,
                &fetch_sig(repo, url)?,
                &s.sha256,
                who,
                via,
            )
            .with_context(|| format!("Sigstore bundle of {} ({url})", s.url))?;
            if fresh {
                eprintln!(">> signed by {who} via {via} (Sigstore, Rekor entry {entry})");
            }
        } else if fresh {
            let why = s.unsigned.as_ref().or(pkg.unsigned.as_ref()).map_or("", |w| w.as_str());
            eprintln!(">> unsigned: {why}");
        }
        out.push((rel, dest));
    }
    Ok(out)
}

fn download(url: &str, dest: &Path, quiet: bool) -> Result<()> {
    fs::create_dir_all(dest.parent().context("parent")?)?;
    let tmp = dest.with_extension("part");
    let mut curl = Command::new("curl");
    if quiet {
        curl.arg("-sS");
    }
    run(curl.args(["-fL", "--retry", "3", "-o"]).arg(&tmp).arg(url))?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// Check `file` against `s.sig` and the key it names; returns the signing
/// key's fingerprint. Signatures and sums lists are fetched once, to
/// build/fetch/sig/<host>/<path>.
fn verify_sig(repo: &Path, s: &Source, file: &Path) -> Result<String> {
    let (Some(sig_url), Some(key)) = (&s.sig, &s.key) else { bail!("no sig") };
    let keys = fs::read(key_file(repo, key))?;
    let cached = |url: &str| fetch_sig(repo, url);
    let sig = cached(sig_url)?;
    match (&s.sums, s.decompress.as_deref()) {
        (Some(sums_url), _) => {
            let sums = cached(sums_url)?;
            let who = crate::openpgp::verify(&keys, &sig, &sums[..])?;
            let name = s.url.rsplit('/').next().unwrap_or("");
            if !lists(&String::from_utf8_lossy(&sums), name, &s.sha256) {
                bail!("{sums_url} does not list {name} with sha256 {}", s.sha256);
            }
            Ok(who)
        }
        (None, Some(_)) => {
            let mut xz = Command::new("xz").arg("-dc").arg(file).stdout(Stdio::piped()).spawn()?;
            let r = crate::openpgp::verify(&keys, &sig, xz.stdout.take().context("xz")?);
            if !xz.wait()?.success() {
                bail!("xz -dc {} failed", file.display());
            }
            r
        }
        (None, None) => crate::openpgp::verify(&keys, &sig, fs::File::open(file)?),
    }
}

/// A signature, sums list or bundle, fetched once to
/// build/fetch/sig/<host>/<path>.
fn fetch_sig(repo: &Path, url: &str) -> Result<Vec<u8>> {
    let rel = url.split_once("://").map_or(url, |(_, r)| r);
    if rel.contains('?') || rel.split('/').any(|c| c.is_empty() || c == "..") {
        bail!("bad signature URL {url}");
    }
    let p = repo.join("build/fetch/sig").join(rel);
    if !p.is_file() {
        download(url, &p, true)?;
    }
    Ok(fs::read(p)?)
}

/// Whether a `sha256sum` list names `name` with `sha256`.
fn lists(sums: &str, name: &str, sha256: &str) -> bool {
    sums.lines().any(|l| {
        let mut f = l.split_whitespace();
        matches!((f.next(), f.next()), (Some(h), Some(n))
            if h.eq_ignore_ascii_case(sha256) && n.trim_start_matches('*') == name)
    })
}

fn sha256(p: &Path) -> Result<String> {
    crate::util::sha256_file(p)
}

/// Topological order of `names` and everything they build or run with.
/// Tools that build each other (the base packs; glibc and python) form a
/// cycle; a cycle through a package the lock already has is broken by
/// using the locked product, so the name asked for is built last
/// (`cargo make pkgs glibc` builds python first, with the locked glibc).
fn order(all: &BTreeMap<String, PkgToml>, names: &[String], locked: &Lock) -> Result<Vec<String>> {
    fn visit(
        n: &str,
        all: &BTreeMap<String, PkgToml>,
        locked: &Lock,
        seen: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Result<()> {
        if seen.contains(n) {
            return Ok(());
        }
        if stack.iter().any(|s| s == n) {
            if locked.contains_key(n) {
                return Ok(());
            }
            bail!("package cycle through {n} (`cargo make bootstrap` makes the first of each base pack)");
        }
        let p = all.get(n).with_context(|| format!("no pkgs/{n}/pkg.toml"))?;
        stack.push(n.to_string());
        for d in p.build.iter().chain(p.run.iter()) {
            // A package may build with an earlier build of itself (gcc,
            // make); that comes from the lock, not a recursion. So does a
            // base pack another package builds with: each base pack is
            // rebuilt in its own turn.
            let base = BASE.contains(&d.as_str()) && locked.contains_key(d);
            if d != n && !base {
                visit(d, all, locked, seen, stack, out)?;
            }
        }
        stack.pop();
        seen.insert(n.to_string());
        out.push(n.to_string());
        Ok(())
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for n in names {
        visit(n, all, locked, &mut seen, &mut Vec::new(), &mut out)?;
    }
    Ok(out)
}

fn components(name: &str, p: &PkgToml) -> serde_json::Value {
    let purl = if p.purl.is_empty() {
        format!("pkg:generic/{name}@{}", p.version)
    } else {
        p.purl.clone()
    };
    json!([{ "name": name, "version": p.version, "purl": purl }])
}

/// Hash a source pack into the store: the verbatim tarballs under `src/`,
/// and the package's own extra files and directories (configs, patches)
/// under `files/`.
fn source_pack(repo: &Path, cat: &Path, name: &str, files: &[(String, PathBuf)]) -> Result<String> {
    let stage = repo.join("build/pkgs/src").join(name);
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(stage.join("src"))?;
    fs::create_dir_all(stage.join("files"))?;
    for (rel, f) in files {
        let to = stage.join("src").join(rel);
        fs::create_dir_all(to.parent().context("parent")?)?;
        fs::copy(f, to)?;
    }
    for e in fs::read_dir(pkgs_dir(repo).join(name))? {
        let e = e?;
        let n = e.file_name().to_string_lossy().into_owned();
        let to = stage.join("files").join(&n);
        if e.path().is_dir() {
            crate::util::copy_tree(&e.path(), &to)?;
        } else if e.path().is_file() && n != "pkg.toml" && n != "build.sh" && !n.ends_with(".plan")
        {
            fs::copy(e.path(), to)?;
        }
    }
    store(cat, &format!("{name}-src"), &stage)
}

/// Put a pack tree into the build catalog's store; returns its hash.
pub fn store(cat: &Path, name: &str, tree: &Path) -> Result<String> {
    let hash = oath_core::hash_tree(tree).map_err(|e| anyhow::anyhow!("{e}"))?;
    oath_core::put_tree(&cat.join("store/pkg").join(name), &hash, tree)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(hash)
}

pub(crate) fn product(lock: &Lock, name: &str) -> Result<String> {
    lock.get(name)
        .map(|e| e.product.clone())
        .with_context(|| format!("pkg:{name} is not built yet (pkgs/lock.json)"))
}

/// `cargo make pkgs [names]`: build in dependency order; with no names,
/// every package.
pub fn build(repo: &Path, names: &[String]) -> Result<()> {
    let all = load(repo)?;
    let cat = build_catalog(repo)?;
    let mut lock = read_lock(repo)?;
    let wanted: Vec<String> =
        if names.is_empty() { all.keys().cloned().collect() } else { names.to_vec() };
    for name in order(&all, &wanted, &lock)? {
        let p = &all[&name];
        let files = fetch(repo, p)?;
        let src = source_pack(repo, &cat, &name, &files)?;
        let mut build_needs = vec![PkgNeed { id: format!("pkg:{name}-src"), hash: src.clone() }];
        for d in build_closure(repo, &all, &lock, &name, p) {
            build_needs.push(PkgNeed { id: format!("pkg:{d}"), hash: product(&lock, &d)? });
        }
        let mut run_needs = Vec::new();
        for d in &p.run {
            run_needs.push(PkgNeed { id: format!("pkg:{d}"), hash: product(&lock, d)? });
        }
        let body = fs::read_to_string(pkgs_dir(repo).join(&name).join("build.sh"))
            .with_context(|| format!("pkgs/{name}/build.sh"))?;
        let files_list: Vec<String> = files.iter().map(|(rel, _)| rel.clone()).collect();
        let script = format!(
            "# generated by cargo make pkgs from pkgs/{name}/\n\
             SRC=/oath/store/pkg/{name}-src/{src}/src\n\
             FILES=/oath/store/pkg/{name}-src/{src}/files\n\
             SOURCES='{files}'\n\
             VERSION={version}\n\
             JOBS=$(nproc)\n\
             LIVE=/oath/store/pkg/{name}/live\n\
             export CC=cc CXX=c++ AR=ar RANLIB=ranlib NM=nm STRIP=strip\n\
             export CFLAGS=\"-O2 -pipe\" CXXFLAGS=\"-O2 -pipe\" SOURCE_DATE_EPOCH=0\n\
             CPPFLAGS= LDFLAGS= PKG_CONFIG_PATH=\n\
             for d in /oath/store/pkg/*/live; do\n\
             \tcase $d in */gcc/live | */glibc/live | *-src/live) continue ;; esac\n\
             \t[ -d \"$d/include\" ] && CPPFLAGS=\"$CPPFLAGS -I$d/include\"\n\
             \t[ -d \"$d/lib\" ] && LDFLAGS=\"$LDFLAGS -L$d/lib\"\n\
             \t[ -d \"$d/lib/pkgconfig\" ] && PKG_CONFIG_PATH=\"$d/lib/pkgconfig${{PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}}\"\n\
             done\n\
             export CPPFLAGS LDFLAGS PKG_CONFIG_PATH\n\
             # Hand-written Makefiles often ignore CPPFLAGS.\n\
             CFLAGS=\"$CFLAGS $CPPFLAGS\" CXXFLAGS=\"$CXXFLAGS $CPPFLAGS\"\n\
             mkdir -p /out\n\
             cat > /out/components.json <<'OATH_EOF'\n{comp}\nOATH_EOF\n\
             {body}",
            files = files_list.join(" "),
            version = p.version,
            comp = serde_json::to_string_pretty(&components(&name, p))?,
        );
        let plan = PlanFile {
            name: name.clone(),
            produces: format!("pkg:{name}"),
            build_needs,
            run_needs,
            script,
        };
        let text = fmt_plan(&plan);
        let plan_path = pkgs_dir(repo).join(&name).join(format!("{name}.plan"));
        fs::write(&plan_path, &text)?;
        let plan_hash = oath_core::file_hash(text.as_bytes());
        if let Some(e) = lock.get(&name) {
            if e.by == "plan"
                && e.plan == plan_hash
                && cat.join("store/pkg").join(&name).join(&e.product).is_dir()
            {
                eprintln!("== pkg:{name} {} (cached)", e.product);
                continue;
            }
        }
        eprintln!(
            "== pkg:{name} {} building, log {}",
            p.version,
            oath_core::build_log(&cat, &name).display()
        );
        let r = oath_core::build_file(&cat, &plan_path)
            .map_err(|e| anyhow::anyhow!("pkg:{name}: {e}"))?;
        check_loads(repo, &cat, &all, &lock, &name, &r.product)?;
        eprintln!("== pkg:{name} {}", r.product);
        lock.insert(
            name.clone(),
            LockEntry {
                version: p.version.clone(),
                license: p.license.clone(),
                by: "plan".into(),
                plan: plan_hash,
                product: r.product,
                run: Some(p.run.clone()),
            },
        );
        write_lock(repo, &lock)?;
    }
    Ok(())
}

/// Every library a new product's programs load (`DT_NEEDED`) must be in
/// its own `lib/`, glibc's, or a pack it runs with (`run`, and what those
/// run with): on a machine a program finds its libraries only in `/lib`,
/// which holds what is linked there (R17), or beside itself through an
/// upstream `$ORIGIN` path (glibc's gconv modules). A library that only
/// the build sandbox had (a base pack, a build pack) is an undeclared
/// `run`.
fn check_loads(
    repo: &Path,
    cat: &Path,
    all: &BTreeMap<String, PkgToml>,
    lock: &Lock,
    name: &str,
    product: &str,
) -> Result<()> {
    let store = cat.join("store/pkg");
    let tree = store.join(name).join(product);
    let libs = |t: &Path| -> BTreeSet<String> {
        fs::read_dir(t.join("lib"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let locked = |n: &str| lock.get(n).map(|e| store.join(n).join(&e.product));
    let mut have = libs(&tree);
    let mut seen = BTreeSet::new();
    let mut todo = vec!["glibc".to_string()];
    todo.extend(all[name].run.iter().cloned());
    while let Some(d) = todo.pop() {
        if d == name || !seen.insert(d.clone()) {
            continue;
        }
        if let Some(t) = locked(&d) {
            have.extend(libs(&t));
        }
        todo.extend(locked_run(repo, all, lock, &d));
    }
    let mut missing: BTreeMap<String, String> = BTreeMap::new();
    let mut files = vec![tree.clone()];
    while let Some(p) = files.pop() {
        let meta = fs::symlink_metadata(&p)?;
        if meta.is_dir() {
            files.extend(fs::read_dir(&p)?.map(|e| e.map(|e| e.path())).collect::<Result<Vec<_>, _>>()?);
            continue;
        }
        if !meta.is_file() || meta.len() < 64 {
            continue;
        }
        let mut head = [0u8; 64];
        std::io::Read::read_exact(&mut fs::File::open(&p)?, &mut head)?;
        if !crate::elf::is_loadable(&head) {
            continue;
        }
        let needs = crate::elf::needs(&fs::read(&p)?)
            .with_context(|| format!("pkg:{name}: {}", p.display()))?;
        let dir = p.parent().unwrap_or(&tree);
        let beside = |so: &str| {
            needs.runpath.iter().any(|r| {
                let r = r.replace("${ORIGIN}", "$ORIGIN");
                r.strip_prefix("$ORIGIN")
                    .is_some_and(|rest| dir.join(rest.trim_start_matches('/')).join(so).exists())
            })
        };
        for so in &needs.needed {
            if !have.contains(so) && !beside(so) {
                let rel = p.strip_prefix(&tree).unwrap_or(&p).display().to_string();
                missing.entry(so.clone()).or_insert(rel);
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let mut msg = format!("pkg:{name} {product} loads libraries none of its run packs has:");
    for (so, file) in &missing {
        let from: Vec<String> = lock
            .keys()
            .filter(|n| locked(n).is_some_and(|t| libs(&t).contains(so)))
            .map(|n| format!("pkg:{n}"))
            .collect();
        let from = if from.is_empty() { "no locked pack".into() } else { from.join(", ") };
        msg.push_str(&format!("\n  {so} ({file}); in {from}"));
    }
    msg.push_str(&format!("\nadd the pack to `run` in pkgs/{name}/pkg.toml"));
    bail!(msg)
}

/// `cargo make pkgs --list`: every package, its lock entry and prose.
pub fn list(repo: &Path) -> Result<()> {
    let lock = read_lock(repo)?;
    for (name, p) in load(repo)? {
        let (by, product) = match lock.get(&name) {
            Some(e) => (e.by.as_str(), &e.product[..e.product.len().min(19)]),
            None => ("-", "not built"),
        };
        println!("{name:<12} {:<9} {by:<9} {product:<19} {}", p.version, p.license);
        if !p.description.is_empty() {
            println!("{:12} {} {}", "", p.description, p.home);
        }
        let keys: BTreeSet<String> = p
            .source
            .iter()
            .filter_map(|s| {
                s.key
                    .as_ref()
                    .map(|k| format!("keys/{k}"))
                    .or_else(|| s.identity.as_ref().map(|i| format!("sigstore:{i}")))
            })
            .collect();
        let unsigned: Vec<String> = p
            .source
            .iter()
            .filter(|s| s.sig.is_none() && s.sigstore.is_none())
            .map(Source::file)
            .collect();
        let mut sig = Vec::new();
        if !keys.is_empty() {
            sig.push(format!("signed by {}", keys.into_iter().collect::<Vec<_>>().join(", ")));
        }
        match unsigned.len() {
            0 => {}
            1..=2 => sig.push(format!("unsigned: {}", unsigned.join(", "))),
            n => sig.push(format!("{n} sources unsigned")),
        }
        println!("{:12} {}", "", sig.join("; "));
    }
    Ok(())
}

/// `cargo make bootstrap`: stage 0 and stage 1 on the host, then store the
/// base packs (stage 1) and lock them.
pub fn bootstrap(repo: &Path, jobs: usize) -> Result<()> {
    let all = load(repo)?;
    let cat = build_catalog(repo)?;
    let get = |name: &str| all.get(name).with_context(|| format!("pkgs/{name}/pkg.toml"));
    let mut files = Vec::new();
    for n in ["gcc", "glibc", "gmp", "mpfr", "mpc"] {
        files.extend(fetch(repo, get(n)?)?);
    }
    let find = |prefix: &str| -> Result<PathBuf> {
        files
            .iter()
            .find(|(rel, _)| rel.starts_with(prefix))
            .map(|(_, p)| p.clone())
            .with_context(|| format!("no toolchain package has a {prefix}* source"))
    };
    let work = repo.join("build/bootstrap/work");
    let out = repo.join("build/bootstrap/out");
    let mut env = format!(
        "GCC_SRC={} BINUTILS_SRC={} GMP_SRC={} MPFR_SRC={} MPC_SRC={} GLIBC_SRC={} LINUX_SRC={}",
        find("gcc-")?.display(),
        find("binutils-")?.display(),
        find("gmp-")?.display(),
        find("mpfr-")?.display(),
        find("mpc-")?.display(),
        find("glibc-")?.display(),
        find("linux-")?.display(),
    );
    // Each stage-1 tool's first source is its release tarball.
    let tools: Vec<&str> = BASE.iter().copied().filter(|n| !["gcc", "glibc"].contains(n)).collect();
    for n in &tools {
        let src = fetch(repo, get(n)?)?.remove(0).1;
        env.push_str(&format!(" SRC_{n}={}", src.display()));
    }
    let script = format!(
        "env {env} TOOLS='{}' WORK={} OUT={} JOBS={jobs} sh {}",
        tools.join(" "),
        work.display(),
        out.display(),
        pkgs_dir(repo).join("bootstrap.sh").display(),
    );
    run(Command::new("nice")
        .args(["-n", "10", "nix-shell"])
        .arg(pkgs_dir(repo).join("bootstrap.nix"))
        .args(["--run", &script]))?;
    let mut lock = read_lock(repo)?;
    for name in BASE {
        let p = get(name)?;
        let tree = out.join(name);
        let comp = serde_json::to_string_pretty(&components(name, p))? + "\n";
        fs::write(tree.join("components.json"), comp)?;
        let hash = store(&cat, name, &tree)?;
        eprintln!("== pkg:{name} {hash} (bootstrap)");
        // The base starts over from these; stage 2 replaces each with a
        // plan product.
        lock.insert(
            name.to_string(),
            LockEntry {
                version: p.version.clone(),
                license: p.license.clone(),
                by: "bootstrap".into(),
                plan: String::new(),
                product: hash,
                run: Some(Vec::new()),
            },
        );
    }
    write_lock(repo, &lock)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(build: &[&str]) -> PkgToml {
        PkgToml {
            version: "1".into(),
            license: "MIT".into(),
            build: build.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn order_puts_deps_first_and_allows_self_builds() {
        let mut all = BTreeMap::new();
        all.insert("gcc".to_string(), pkg(&[]));
        all.insert("bash".to_string(), pkg(&["gcc", "bash"]));
        all.insert("zlib".to_string(), pkg(&["gcc", "bash"]));
        all.insert("btrfs".to_string(), pkg(&["gcc", "bash", "zlib"]));
        let none = Lock::new();
        let o = order(&all, &["btrfs".into()], &none).unwrap();
        assert_eq!(o, vec!["gcc", "bash", "zlib", "btrfs"]);
        all.insert("a".to_string(), pkg(&["b"]));
        all.insert("b".to_string(), pkg(&["a"]));
        assert!(order(&all, &["a".into()], &none).is_err());
        let mut locked = Lock::new();
        locked.insert("a".into(), LockEntry::default());
        assert_eq!(order(&all, &["a".into()], &locked).unwrap(), vec!["b", "a"]);
    }

    #[test]
    fn a_build_tool_brings_what_it_runs_with() {
        let mut all = BTreeMap::new();
        let mut bison = pkg(&["gcc"]);
        bison.run = vec!["m4".into()];
        all.insert("bison".to_string(), bison);
        all.insert("m4".to_string(), pkg(&["gcc"]));
        all.insert("gcc".to_string(), pkg(&[]));
        let glibc = pkg(&["gcc", "bison"]);
        let mut lock = Lock::new();
        let repo = Path::new("/nonexistent");
        let closure = |all: &BTreeMap<String, PkgToml>, lock: &Lock, name: &str, p: &PkgToml| {
            build_closure(repo, all, lock, name, p)
        };
        assert_eq!(closure(&all, &lock, "glibc", &glibc), ["gcc", "bison", "m4"]);
        // A bootstrap pack brings nothing: it carries what it runs with.
        all.get_mut("gcc").unwrap().run = vec!["gmp".into()];
        let entry = |by: &str, run: Option<Vec<String>>| LockEntry {
            by: by.into(),
            run,
            ..Default::default()
        };
        lock.insert("gcc".into(), entry("bootstrap", Some(Vec::new())));
        assert_eq!(closure(&all, &lock, "glibc", &glibc), ["gcc", "bison", "m4"]);
        lock.insert("gcc".into(), entry("plan", Some(vec!["gmp".into()])));
        assert_eq!(closure(&all, &lock, "glibc", &glibc), ["gcc", "gmp", "bison", "m4"]);
        // What the locked product was built with counts, not pkg.toml now;
        // and a package's first build cannot need itself.
        lock.insert("gcc".into(), entry("plan", Some(Vec::new())));
        assert_eq!(closure(&all, &lock, "glibc", &glibc), ["gcc", "bison", "m4"]);
        lock.insert("gcc".into(), entry("plan", Some(vec!["gmp".into()])));
        let gmp = pkg(&["gcc"]);
        assert_eq!(closure(&all, &lock, "gmp", &gmp), ["gcc"]);
        lock.insert("gmp".into(), entry("plan", Some(Vec::new())));
        assert_eq!(closure(&all, &lock, "gmp", &gmp), ["gcc", "gmp"]);
    }

    #[test]
    fn sums_lists_name_and_hash() {
        let sums = "abcd  other.tar.gz\nBD24  cmake-4.4.4.tar.gz\nffff *bin.tar.gz\n";
        assert!(lists(sums, "cmake-4.4.4.tar.gz", "bd24"));
        assert!(lists(sums, "bin.tar.gz", "ffff"));
        assert!(!lists(sums, "cmake-4.4.4.tar.gz", "abcd"));
        assert!(!lists(sums, "cmake-4.4.4.tar", "bd24"));
    }

    #[test]
    fn repo_packages_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let all = load(&repo).unwrap();
        for (name, p) in &all {
            for s in &p.source {
                assert_eq!(s.sha256.len(), 64, "{name}: {}", s.url);
            }
            assert!(pkgs_dir(&repo).join(name).join("build.sh").is_file(), "pkgs/{name}/build.sh");
        }
        // Every package builds with the base, the base packs with
        // earlier builds of themselves.
        for (name, p) in &all {
            assert_eq!(p.build[..BASE.len()], *BASE, "pkgs/{name}");
        }
    }
}
