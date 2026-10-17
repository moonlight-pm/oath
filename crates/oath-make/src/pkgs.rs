//! Native packages. `pkgs/<name>/pkg.toml` names a package's upstream
//! sources (pinned by sha256), its licence and the packs it builds with;
//! `pkgs/<name>/build.sh` is the build. `cargo make pkgs` turns each into
//! an `oath build` plan pinned to its source pack and to the exact build
//! packs in `pkgs/lock.json`, writes the plan to `pkgs/<name>/<name>.plan`,
//! and builds it in the sandbox. `cargo make bootstrap` makes the first
//! three packs (llvm, busybox, make) with `pkgs/bootstrap.sh` on the host;
//! after that their plans rebuild them with themselves.
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
    /// Package URL for components.json (`pkg:generic/busybox@1.38.0`).
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
        let p: PkgToml =
            toml::from_str(&text).with_context(|| format!("parse {}", toml_p.display()))?;
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
/// Tools that build each other (busybox and make; llvm and python) form a
/// cycle; a cycle through a package the lock already has is broken by
/// using the locked product, so the name asked for is built last
/// (`cargo make pkgs llvm` builds python first, with the locked llvm).
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
            bail!("package cycle through {n} (`cargo make bootstrap` makes the first llvm, busybox and make)");
        }
        let p = all.get(n).with_context(|| format!("no pkgs/{n}/pkg.toml"))?;
        stack.push(n.to_string());
        for d in p.build.iter().chain(p.run.iter()) {
            // A package may build with an earlier build of itself
            // (busybox, make); that comes from the lock, not a recursion.
            if d != n {
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
        for d in &p.build {
            build_needs.push(PkgNeed { id: format!("pkg:{d}"), hash: product(&lock, d)? });
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
             \tcase $d in */llvm/live | *-src/live) continue ;; esac\n\
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
        eprintln!("== pkg:{name} {}", r.product);
        lock.insert(
            name.clone(),
            LockEntry {
                version: p.version.clone(),
                license: p.license.clone(),
                by: "plan".into(),
                plan: plan_hash,
                product: r.product,
            },
        );
        write_lock(repo, &lock)?;
    }
    Ok(())
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

/// `cargo make bootstrap`: stage 0 and stage 1 on the host, then store
/// pkg:llvm, pkg:busybox and pkg:make (stage 1) and lock them.
pub fn bootstrap(repo: &Path, jobs: usize) -> Result<()> {
    let all = load(repo)?;
    let cat = build_catalog(repo)?;
    let llvm = all.get("llvm").context("pkgs/llvm/pkg.toml")?;
    let busybox = all.get("busybox").context("pkgs/busybox/pkg.toml")?;
    let make = all.get("make").context("pkgs/make/pkg.toml")?;
    let llvm_files = fetch(repo, llvm)?;
    let find = |prefix: &str| -> Result<PathBuf> {
        llvm_files
            .iter()
            .find(|(rel, _)| rel.starts_with(prefix))
            .map(|(_, p)| p.clone())
            .with_context(|| format!("pkgs/llvm/pkg.toml has no {prefix}* source"))
    };
    let busybox_src = fetch(repo, busybox)?.remove(0).1;
    let make_src = fetch(repo, make)?.remove(0).1;
    let work = repo.join("build/bootstrap/work");
    let out = repo.join("build/bootstrap/out");
    let script = format!(
        "env LLVM_SRC={} MUSL_SRC={} LINUX_SRC={} BUSYBOX_SRC={} MAKE_SRC={} WORK={} OUT={} JOBS={jobs} sh {}",
        find("llvm-project")?.display(),
        find("musl")?.display(),
        find("linux")?.display(),
        busybox_src.display(),
        make_src.display(),
        work.display(),
        out.display(),
        pkgs_dir(repo).join("bootstrap.sh").display(),
    );
    run(Command::new("nice")
        .args(["-n", "10", "nix-shell"])
        .arg(pkgs_dir(repo).join("bootstrap.nix"))
        .args(["--run", &script]))?;
    let mut lock = read_lock(repo)?;
    for (name, p) in [("llvm", llvm), ("busybox", busybox), ("make", make)] {
        let tree = out.join(name);
        let comp = serde_json::to_string_pretty(&components(name, p))? + "\n";
        fs::write(tree.join("components.json"), comp)?;
        let hash = store(&cat, name, &tree)?;
        eprintln!("== pkg:{name} {hash} (bootstrap)");
        // A stage-2 build of the same name replaces a bootstrap entry.
        let keep = lock.get(name).is_some_and(|e| e.by == "plan");
        if !keep {
            lock.insert(
                name.to_string(),
                LockEntry {
                    version: p.version.clone(),
                    license: p.license.clone(),
                    by: "bootstrap".into(),
                    plan: String::new(),
                    product: hash,
                },
            );
        }
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
        all.insert("llvm".to_string(), pkg(&[]));
        all.insert("busybox".to_string(), pkg(&["llvm", "busybox"]));
        all.insert("zlib".to_string(), pkg(&["llvm", "busybox"]));
        all.insert("btrfs".to_string(), pkg(&["llvm", "busybox", "zlib"]));
        let none = Lock::new();
        let o = order(&all, &["btrfs".into()], &none).unwrap();
        assert_eq!(o, vec!["llvm", "busybox", "zlib", "btrfs"]);
        all.insert("a".to_string(), pkg(&["b"]));
        all.insert("b".to_string(), pkg(&["a"]));
        assert!(order(&all, &["a".into()], &none).is_err());
        let mut locked = Lock::new();
        locked.insert("a".into(), LockEntry::default());
        assert_eq!(order(&all, &["a".into()], &locked).unwrap(), vec!["b", "a"]);
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
        // The bootstrap's three rebuild themselves.
        for name in ["llvm", "busybox", "make"] {
            assert!(all[name].build.iter().any(|b| b == name), "pkgs/{name} builds with itself");
        }
    }
}
