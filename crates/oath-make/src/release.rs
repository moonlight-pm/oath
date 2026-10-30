//! An output as a release (R38): the bundle a machine's `oath update`
//! fetches (`out/<id>/update.tar`) and the release that says what it is
//! (`out/<id>.json`), signed with this build host's key
//! (`~/.config/oath/sign_ed25519`, `ssh-keygen -Y sign`), then
//! `out/<profile>.json`, the newest for its profile.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use oath_core::update::{self as up, Bundle, Release};
use serde_json::Value;

use crate::output::Output;
use crate::util::run_out;

/// The signing key: `$OATH_SIGN_KEY`, else `sign_ed25519` beside
/// `origin.env`.
pub fn sign_key() -> PathBuf {
    std::env::var_os("OATH_SIGN_KEY")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::origin::config_file().with_file_name("sign_ed25519"))
}

/// This host's public signing key (`ssh-ed25519 AAAA… name`).
pub fn public_key() -> Result<String> {
    let key = sign_key();
    let pub_file = PathBuf::from(format!("{}.pub", key.display()));
    let line = fs::read_to_string(&pub_file).with_context(|| {
        format!(
            "no signing key at {}: ssh-keygen -t ed25519 -N '' -C oath@$(hostname) -f {} \
             (machines trust it once a person adds it to host:local.publishers)",
            key.display(),
            key.display()
        )
    })?;
    Ok(line.trim().to_string())
}

/// A release made in a directory: `update.tar`, `release.json` and its
/// `.sig`.
pub struct Made {
    pub output: String,
    pub profile: String,
    pub bundle: PathBuf,
    pub release: PathBuf,
    pub sig: PathBuf,
}

impl Made {
    /// `(key, file)` in upload order: the profile's newest goes last.
    pub fn files(&self) -> Vec<(String, PathBuf)> {
        let sig = |k: String| format!("{k}.sig");
        let (id, name) = (up::release_key(&self.output), up::release_key(&self.profile));
        vec![
            (up::bundle_key(&self.output), self.bundle.clone()),
            (sig(id.clone()), self.sig.clone()),
            (id, self.release.clone()),
            (sig(name.clone()), self.sig.clone()),
            (name, self.release.clone()),
        ]
    }
}

/// Bundle, release and signature for output `o`, in `dir`.
pub fn make(o: &Output, dir: &Path) -> Result<Made> {
    let manifest: Value = serde_json::from_slice(&fs::read(o.file("manifest.json"))?)?;
    let profile = manifest["profile"].as_str().context("the manifest names no profile")?;
    let key = sign_key();
    let public = public_key()?;
    fs::create_dir_all(dir)?;
    let bundle = dir.join("update.tar");
    write_bundle(o, &bundle)?;
    let bytes = fs::read(&bundle)?;
    let release = Release {
        output: o.id.clone(),
        profile: profile.to_string(),
        published: oath_core::now_rfc3339(),
        committed: committed(&manifest),
        bundle: Bundle { hash: oath_core::file_hash(&bytes), size: bytes.len() as u64 },
        init: oath_core::file_hash(&fs::read(o.file("stage/lib/oath/init"))?),
        pinned: pinned(&manifest),
    };
    let path = dir.join("release.json");
    let mut text = serde_json::to_string_pretty(&release)?;
    text.push('\n');
    fs::write(&path, &text)?;
    let sig = dir.join("release.json.sig");
    let _ = fs::remove_file(&sig);
    run_out(
        Command::new("ssh-keygen")
            .args(["-Y", "sign", "-q", "-n", up::NAMESPACE, "-f"])
            .arg(&key)
            .arg(&path),
    )
    .context("ssh-keygen -Y sign")?;
    // The signature a machine will check, checked here first.
    let who = oath_core::sshsig::verify(
        text.as_bytes(),
        &fs::read_to_string(&sig)?,
        up::NAMESPACE,
        &[public],
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "release {} ({profile}, {:.1} MB bundle) signed by {who}",
        o.id,
        bytes.len() as f64 / 1e6
    );
    Ok(Made { output: o.id.clone(), profile: profile.to_string(), bundle, release: path, sig })
}

/// When the commit pkg:oath in `manifest` was built from was made (RFC
/// 3339, UTC), as this checkout's git knows it: the order of outputs
/// (`oath_core::update::newer`). None when git does not have the commit.
pub(crate) fn committed(manifest: &Value) -> Option<String> {
    let commit = manifest["packs"]["oath"]["commit"].as_str()?.trim_end_matches("-dirty");
    let out = run_out(
        Command::new("git")
            .arg("-C")
            .arg(crate::util::repo_root())
            .args(["show", "-s", "--format=%ct", commit]),
    )
    .ok()?;
    let secs: i64 = out.trim().parse().ok()?;
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// The output's pins of the packs it pins from elsewhere (R28): a machine
/// still at one moves with the next output.
pub(crate) fn pinned(manifest: &Value) -> BTreeMap<String, String> {
    manifest["pinned"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let n = n.as_str()?;
            Some((n.to_string(), manifest["packs"][n]["hash"].as_str()?.to_string()))
        })
        .collect()
}

/// The bundle: the manifest, the boot files, the seed's objects (desired
/// and meta only) and plans, and the image's own files, as a tar with no
/// times or owners, so the same output makes the same file.
fn write_bundle(o: &Output, dest: &Path) -> Result<()> {
    let mut list =
        vec!["manifest.json".to_string(), up::KERNEL.into(), up::INITRD.into(), up::EFI.into()];
    if o.file("stage/oath/store/plan").is_dir() {
        list.push("stage/oath/store/plan".into());
    }
    let objects = o.file("stage/oath/objects");
    for kind in fs::read_dir(&objects)? {
        let kind = kind?;
        if !kind.path().is_dir() {
            continue;
        }
        for obj in fs::read_dir(kind.path())? {
            let obj = obj?;
            for f in ["desired.json", "meta.json"] {
                let p = obj.path().join(f);
                if p.is_file() {
                    list.push(p.strip_prefix(&o.dir)?.display().to_string());
                }
            }
        }
    }
    for f in up::ROOT_FILES {
        if o.dir.join("stage").join(f).symlink_metadata().is_ok() {
            list.push(format!("stage/{f}"));
        }
    }
    list.sort();
    let names = dest.with_extension("list");
    fs::write(&names, list.join("\n") + "\n")?;
    run_out(
        Command::new("tar")
            .args(["--sort=name", "--mtime=@0", "--owner=0", "--group=0", "--numeric-owner"])
            .args(["--format=gnu", "-cf"])
            .arg(dest)
            .arg("-C")
            .arg(&o.dir)
            .arg("-T")
            .arg(&names),
    )
    .context("tar the bundle")?;
    let _ = fs::remove_file(&names);
    Ok(())
}

/// The packs output `o` ships in its root filesystem, as origin keys: a
/// machine fetches each one it lacks.
pub fn pack_keys(o: &Output) -> Result<BTreeSet<String>> {
    let manifest: Value = serde_json::from_slice(&fs::read(o.file("manifest.json"))?)?;
    Ok(manifest["packs"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, p)| p["ships_in"].as_array().is_some_and(|s| s.contains(&"rootfs".into())))
        .filter_map(|(n, p)| Some(format!("pkg/{n}/{}.tar", p["hash"].as_str()?)))
        .collect())
}

/// Publish output `o`'s release after its packs, to the default origin or
/// into `dest`. An output already there stays as it is (its release keeps
/// its time), and the profile's newest does not move back to it.
pub fn publish(o: &Output, dest: Option<&Path>, tmp: &Path) -> Result<()> {
    crate::check::require_checked(&o.dir)?;
    let origin = match dest {
        None => Some(crate::origin::Origin::from_env()?),
        Some(_) => None,
    };
    let keys = match &origin {
        Some(origin) => origin.keys()?,
        None => BTreeSet::new(),
    };
    let there = |key: &str| keys.contains(key) || dest.is_some_and(|d| d.join(key).is_file());
    if there(&up::release_key(&o.id)) {
        println!("output {} is already published", o.id);
        return Ok(());
    }
    let mut missing = Vec::new();
    for key in pack_keys(o)? {
        if !there(&key) {
            missing.push(key);
        }
    }
    if !missing.is_empty() {
        bail!(
            "output {} ships packs the origin does not have, so a machine could not fetch \
             them:\n  {}\n(a pack pinned from elsewhere is its owner's to publish)",
            o.id,
            missing.join("\n  ")
        );
    }
    let made = make(o, tmp)?;
    for (key, file) in made.files() {
        match (&origin, dest) {
            (Some(origin), _) => origin.put(&key, &file)?,
            (None, Some(d)) => {
                let to = d.join(&key);
                fs::create_dir_all(to.parent().context("parent")?)?;
                fs::copy(&file, &to).with_context(|| format!("write {}", to.display()))?;
            }
            (None, None) => unreachable!(),
        }
        println!("put {key}");
    }
    println!("output {} is the newest for {}", made.output, made.profile);
    Ok(())
}
