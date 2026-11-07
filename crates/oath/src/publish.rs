//! `oath publish pkg:<name>` (R72): put this machine's pin of a package on
//! an origin as its head, so every machine whose `host:local.publishers`
//! lets this key sign that name takes it at its next `oath update`.
//!
//! The pin must be applied (its needs were checked here) and built from a
//! clean tree (`source.dirty` is false). The head is `pkg/<name>.json`,
//! signed beside itself with `~/.config/oath/sign_ed25519` (namespace
//! `oath-pkg`), and checked against that key's `.pub` before it leaves.
//! The pack goes up first when the origin lacks it, then the signature,
//! then the head. A head older than the one the origin serves, by commit
//! time, is refused. Without `--confirm` it says what it would publish.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oath_core::head::{self, Head};
use oath_core::origin::{self, Origin};
use oath_core::update::short;
use oath_core::{find_bin, Catalog, Error, ObjectId, PkgNeed, Result, KIND_PKG};
use serde_json::{json, Value};

pub struct Opts {
    pub id: String,
    /// Write the origin's tree into this directory instead of uploading.
    pub dest: Option<PathBuf>,
    /// When the commit the tree was built from was made (RFC 3339): the
    /// head's order against the one it replaces.
    pub committed: Option<String>,
    pub confirm: bool,
    pub json: bool,
}

pub fn run(root: &Path, cat: &Catalog, opts: Opts) -> Result<i32> {
    let id: ObjectId = opts.id.parse()?;
    if id.kind != KIND_PKG {
        return Err(Error::hint("oath publish takes pkg:<name>", "oath publish pkg:<name>"));
    }
    let name = id.name.clone();
    if cat.diff(Some(&id))?.iter().any(|d| d.id == id) {
        return Err(Error::hint(
            format!("{id} has changes not applied"),
            format!("`sudo oath apply {id}` first: a head is a pin this machine runs"),
        ));
    }
    let doc = cat.get(&id)?.desired;
    let hash = doc["hash"].as_str().unwrap_or("").to_string();
    let tree = oath_core::realization_dir(root, &name, &hash);
    if hash.is_empty() || !tree.is_dir() {
        return Err(Error::hint(
            format!("{id} has no pinned tree in this store"),
            "oath store pkg:<name> <dir>, oath set pkg:<name> hash=…, oath apply",
        ));
    }
    let source = doc.get("source").filter(|s| !s.is_null()).cloned();
    if source.as_ref().is_some_and(|s| s["dirty"] == json!(true)) {
        return Err(Error::hint(
            format!("{id} was built from a tree with changes not committed"),
            "commit them, build again and pin that: a head says which commit it is",
        ));
    }
    if let Some(c) = &opts.committed {
        if !oath_core::is_rfc3339(c) {
            return Err(Error::hint(
                format!("--committed {c:?}"),
                "an RFC 3339 time: 2026-11-06T12:00:00Z",
            ));
        }
    }
    let needs: Vec<PkgNeed> = serde_json::from_value(doc["needs"].clone()).unwrap_or_default();
    let s = |k: &str| doc[k].as_str().unwrap_or("").to_string();
    let head = Head {
        name: name.clone(),
        hash: hash.clone(),
        needs,
        published: oath_core::now_rfc3339(),
        committed: opts.committed.clone(),
        source,
        version: oath_core::read_components(&tree)
            .and_then(|c| oath_core::own_version(&c, &name))
            .unwrap_or_default(),
        description: s("description"),
        home: s("home"),
        category: s("category"),
    };
    let key = origin::public_key()?;
    let signer = key.split_whitespace().nth(2).unwrap_or("").to_string();
    let to = match &opts.dest {
        Some(d) => d.display().to_string(),
        None => "the default origin".to_string(),
    };
    // The head the origin serves now, from where machines read it.
    let replaces = match &opts.dest {
        Some(d) => fs::read(d.join(head::key(&name))).ok(),
        None => first_origin(cat).and_then(|o| read_url(&format!("{o}/{}", head::key(&name)))),
    }
    .and_then(|b| serde_json::from_slice::<Head>(&b).ok());
    if let Some(old) = &replaces {
        if old.hash == head.hash {
            let v = json!({ "id": id.to_string(), "hash": hash, "published": false });
            return finish(&opts, v, &format!("{id} {} is its head on {to} already", short(&hash)));
        }
        if let (Some(a), Some(b)) = (&old.committed, &head.committed) {
            if b < a {
                return Err(Error::hint(
                    format!(
                        "{to} serves a head of {id} from a later commit ({a}) than this one ({b})"
                    ),
                    "publish a build of that commit or a later one",
                ));
            }
        }
    }
    let what = format!(
        "{id} {}{} as its head on {to}, signed by {signer}{}",
        short(&hash),
        if head.version.is_empty() { String::new() } else { format!(" ({})", head.version) },
        replaces.as_ref().map(|o| format!(", replacing {}", short(&o.hash))).unwrap_or_default()
    );
    if !opts.confirm {
        let v = json!({ "id": id.to_string(), "head": head, "to": to, "published": false });
        let text = format!("would publish {what}; `oath publish --confirm {id}` does");
        return finish(&opts, v, &text);
    }

    let work = Scratch::new()?;
    let file = work.path().join(format!("{name}.json"));
    let mut text = serde_json::to_string_pretty(&head)?;
    text.push('\n');
    fs::write(&file, &text)?;
    let sig = origin::sign(&file, head::NAMESPACE)?;
    // The signature every machine will check, checked here first.
    oath_core::sshsig::verify_for(
        text.as_bytes(),
        &fs::read_to_string(&sig)?,
        head::NAMESPACE,
        std::slice::from_ref(&key),
        &name,
    )?;
    let pack_key = format!("pkg/{name}/{hash}.tar");
    let sig_key = format!("{}.sig", head::key(&name));
    match &opts.dest {
        Some(d) => {
            let pack = d.join(&pack_key);
            if !pack.is_file() {
                write_tar(&tree, &pack)?;
            }
            for (k, f) in [(&sig_key, &sig), (&head::key(&name), &file)] {
                let to = d.join(k);
                fs::create_dir_all(to.parent().unwrap())?;
                fs::copy(f, to)?;
            }
        }
        None => {
            let o = Origin::from_env()?;
            if !o.keys_under(&format!("pkg/{name}/"))?.contains(&pack_key) {
                let pack = work.path().join("pack.tar");
                write_tar(&tree, &pack)?;
                eprintln!("put {pack_key} ({:.1} MB)", fs::metadata(&pack)?.len() as f64 / 1e6);
                o.put(&pack_key, &pack)?;
            }
            o.put(&sig_key, &sig)?;
            o.put(&head::key(&name), &file)?;
        }
    }
    oath_core::tel("oath", "publish", json!({ "id": id.to_string(), "hash": hash, "to": to }));
    let v = json!({ "id": id.to_string(), "head": head, "to": to, "published": true });
    finish(&opts, v, &format!("published {what}"))
}

/// A directory of its own under the system's temporary one, removed when
/// the run ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        let d = std::env::temp_dir().join(format!("oath-publish-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir(&d)?;
        Ok(Self(d))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The first of `host:local.origins`: where machines read heads first.
fn first_origin(cat: &Catalog) -> Option<String> {
    let host = cat.get(&ObjectId::new("host", "local")).ok()?.desired;
    host["origins"].as_array()?.first()?.as_str().map(|o| o.trim_end_matches('/').to_string())
}

fn read_url(url: &str) -> Option<Vec<u8>> {
    let out = Command::new(find_bin("curl")).args(["-fsSL", url]).output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// A pack: the tree as a tar, which apply unpacks and hashes again.
fn write_tar(tree: &Path, tar: &Path) -> Result<()> {
    if let Some(p) = tar.parent() {
        fs::create_dir_all(p)?;
    }
    let out = Command::new(find_bin("tar"))
        .arg("-C")
        .arg(tree)
        .arg("-cf")
        .arg(tar)
        .arg(".")
        .output()
        .map_err(|e| Error::Msg(format!("tar: {e}")))?;
    if !out.status.success() {
        return Err(Error::Msg(format!("tar: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(())
}

fn finish(opts: &Opts, v: Value, text: &str) -> Result<i32> {
    if opts.json {
        println!("{v}");
    } else {
        println!("{text}");
    }
    Ok(0)
}
