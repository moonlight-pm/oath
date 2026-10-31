//! Build outputs: `build/out/<id>/`, one per image build. A build packs into
//! `build/out/.<id>.part/` and renames it when complete; after that nothing
//! writes into it except a clean `cargo make check`, which adds
//! `check-pass`. The id is the first 12 hex digits of the SHA-256 of the
//! output's manifest (without the commit), so an unchanged tree reuses its
//! output. `build/out/latest` names the newest. The newest five outputs
//! are kept, and the newest checked one.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use serde_json::Value;

const KEEP: usize = 5;

pub struct Output {
    pub id: String,
    pub dir: PathBuf,
}

impl Output {
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// A clean `cargo make check` passed it (the stamp's hashes are checked
    /// by `check::require_checked`).
    pub fn checked(&self) -> bool {
        self.file("check-pass").is_file()
    }

    /// When it was built (its manifest's mtime; a reuse touches it).
    fn built(&self) -> SystemTime {
        fs::metadata(self.file("manifest.json"))
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

fn root(out: &Path) -> PathBuf {
    out.join("out")
}

/// The id of a manifest: what the output is made of, not the commit
/// (the tree's, pkg:oath's, and a first-party tree's in a pack's
/// `sources`).
pub fn id(manifest: &Value) -> String {
    let mut m = manifest.clone();
    if let Some(top) = m.as_object_mut() {
        top.remove("commit");
    }
    if let Some(oath) = m.pointer_mut("/packs/oath").and_then(Value::as_object_mut) {
        oath.remove("commit");
    }
    if let Some(packs) = m.get_mut("packs").and_then(Value::as_object_mut) {
        for sources in packs.values_mut().filter_map(|p| p.get_mut("sources")) {
            for s in sources.as_array_mut().into_iter().flatten() {
                if let Some(s) = s.as_object_mut() {
                    s.remove("commit");
                }
            }
        }
    }
    let hash = oath_core::file_hash(serde_json::to_string(&m).unwrap_or_default().as_bytes());
    hash.trim_start_matches("sha256-")[..12].to_string()
}

/// The output `id`, if it was built.
pub fn open(out: &Path, id: &str) -> Result<Output> {
    if id.is_empty() || id.contains('/') || id.starts_with('.') || id == "latest" {
        bail!("bad output id {id:?}");
    }
    let dir = root(out).join(id);
    if !dir.join("manifest.json").is_file() {
        bail!("no output {id} in {} (cargo make build)", root(out).display());
    }
    Ok(Output { id: id.to_string(), dir })
}

/// Every complete output, newest first.
fn all(out: &Path) -> Result<Vec<Output>> {
    let Ok(dir) = fs::read_dir(root(out)) else { return Ok(Vec::new()) };
    let mut v: Vec<Output> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && n != "latest")
        .filter_map(|n| open(out, &n).ok())
        .collect();
    v.sort_by_key(|o| std::cmp::Reverse(o.built()));
    Ok(v)
}

pub fn latest(out: &Path) -> Result<Output> {
    let link = root(out).join("latest");
    let id = fs::read_link(&link)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .context("no build output yet: cargo make build")?;
    open(out, &id)
}

pub fn newest_checked(out: &Path) -> Result<Output> {
    all(out)?.into_iter().find(Output::checked).with_context(|| {
        format!("no output in {} has passed `cargo make check`", root(out).display())
    })
}

/// `--output <id>`, else the newest checked output (`checked`) or the
/// latest one.
pub fn pick(out: &Path, id: Option<&str>, checked: bool) -> Result<Output> {
    match id {
        Some(id) => open(out, id),
        None if checked => newest_checked(out),
        None => latest(out),
    }
}

/// An existing output of this id, made the latest again. With `manifest`
/// (this build's, the same but for its commits), the output records the
/// commits it was built again from: a tree built dirty and then committed
/// is clean now, and may be published (R47).
pub fn reuse(out: &Path, id: &str, manifest: Option<&Value>) -> Result<Option<Output>> {
    let Ok(o) = open(out, id) else { return Ok(None) };
    if let Some(m) = manifest {
        crate::util::write_pretty(&o.file("manifest.json"), m)?;
    }
    fs::File::options()
        .write(true)
        .open(o.file("manifest.json"))?
        .set_modified(SystemTime::now())?;
    set_latest(out, id)?;
    Ok(Some(o))
}

/// A fresh directory to pack output `id` into.
pub fn begin(out: &Path, id: &str) -> Result<PathBuf> {
    let work = root(out).join(format!(".{id}.part"));
    if work.exists() {
        fs::remove_dir_all(&work).with_context(|| format!("remove {}", work.display()))?;
    }
    fs::create_dir_all(&work)?;
    Ok(work)
}

/// Make the packed `work` the output `id`, the latest, and prune.
pub fn finish(out: &Path, id: &str, work: &Path) -> Result<Output> {
    let dir = root(out).join(id);
    fs::rename(work, &dir).with_context(|| format!("{} -> {}", work.display(), dir.display()))?;
    set_latest(out, id)?;
    prune(out)?;
    open(out, id)
}

fn set_latest(out: &Path, id: &str) -> Result<()> {
    let link = root(out).join("latest");
    let tmp = root(out).join(".latest");
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(id, &tmp)?;
    fs::rename(&tmp, &link)?;
    Ok(())
}

/// Keep the newest `KEEP` outputs, the latest, and the newest checked one.
fn prune(out: &Path) -> Result<()> {
    let latest = latest(out).ok().map(|o| o.id);
    let checked = newest_checked(out).ok().map(|o| o.id);
    for o in all(out)?.into_iter().skip(KEEP) {
        if Some(&o.id) != latest.as_ref() && Some(&o.id) != checked.as_ref() {
            eprintln!("prune output {}", o.id);
            fs::remove_dir_all(&o.dir).with_context(|| format!("remove {}", o.dir.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make(out: &Path, id: &str, checked: bool) {
        let w = begin(out, id).unwrap();
        fs::write(w.join("manifest.json"), "{}").unwrap();
        if checked {
            fs::write(w.join("check-pass"), "{}").unwrap();
        }
        finish(out, id, &w).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    #[test]
    fn id_ignores_the_commit() {
        let a = json!({"packs": {"oath": {"hash": "h", "commit": "abc"}}});
        let b = json!({"packs": {"oath": {"hash": "h", "commit": "def-dirty"}}});
        let c = json!({"packs": {"oath": {"hash": "other", "commit": "abc"}}});
        assert_eq!(id(&a), id(&b));
        assert_ne!(id(&a), id(&c));
        assert_eq!(id(&a).len(), 12);
        let tree = |commit: &str| json!({"packs": {"rivulet": {"hash": "r", "sources": [{"tree": "apps/rivulet", "commit": commit}]}}});
        assert_eq!(id(&tree("abc")), id(&tree("def")));
    }

    #[test]
    fn latest_checked_and_prune() {
        let t = tempfile::tempdir().unwrap();
        let out = t.path();
        assert!(latest(out).is_err());
        make(out, "aaaa", true);
        for i in 0..6 {
            make(out, &format!("b{i}"), false);
        }
        assert_eq!(latest(out).unwrap().id, "b5");
        // Six unchecked builds after it, and the checked one is still kept.
        assert_eq!(newest_checked(out).unwrap().id, "aaaa");
        assert_eq!(pick(out, None, true).unwrap().id, "aaaa");
        assert_eq!(pick(out, Some("b3"), true).unwrap().id, "b3");
        assert!(open(out, "b0").is_err(), "pruned");
        assert!(open(out, "b1").is_ok());
        // A reused output becomes the latest.
        reuse(out, "b2", None).unwrap();
        assert_eq!(latest(out).unwrap().id, "b2");
        assert!(open(out, "../x").is_err() && open(out, "latest").is_err());
    }
}
