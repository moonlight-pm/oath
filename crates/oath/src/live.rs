use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::sys::reboot::{reboot, RebootMode};
use nix::unistd::{sethostname, sync};
use oath_core::{
    converge_dev, converge_net, converge_pkg, converge_ssh, fetch_url, gen_subvol_name,
    ingest_file, install_tree, store_present, tel, ApplyHooks, Dev, DevActual, Error, Host,
    HostPower, Net, ObjectId, Pkg, PkgActual, Result, Ssh, SshActual, BTRFS_TOP, LIVE_SUBVOL,
};
use serde_json::json;

pub struct Live {
    pub catalog_root: PathBuf,
}

impl Live {
    fn sibling_live() -> PathBuf {
        Path::new(BTRFS_TOP).join(LIVE_SUBVOL)
    }

    fn sibling_gen(generation: u64) -> PathBuf {
        Path::new(BTRFS_TOP).join(gen_subvol_name(generation))
    }

    fn copy_fallback_dir(&self, generation: u64) -> PathBuf {
        self.catalog_root.join(".gens").join(generation.to_string())
    }

    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        if dst.exists() {
            let _ = fs::remove_dir_all(dst);
        }
        copy_recursive(src, dst)
    }

    /// Restore catalog documents without touching `/oath/run` (mounts, socket).
    fn restore_catalog(src_oath: &Path, dst_oath: &Path) -> Result<()> {
        for name in ["objects", "schema", "log", "INDEX.md", "store", "ssh"] {
            let from = src_oath.join(name);
            let to = dst_oath.join(name);
            if from.is_dir() {
                Self::copy_dir(&from, &to)?;
            } else if from.is_file() {
                if let Some(p) = to.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::copy(&from, &to)?;
            }
        }
        Ok(())
    }
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        let from = e.path();
        let meta = from.symlink_metadata()?;
        if meta.file_type().is_dir() {
            copy_recursive(&from, &to)?;
        } else if meta.file_type().is_symlink() {
            let t = fs::read_link(&from)?;
            let _ = fs::remove_file(&to);
            std::os::unix::fs::symlink(&t, &to)
                .map_err(|err| Error::Msg(format!("symlink {}: {err}", to.display())))?;
        } else if meta.file_type().is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

impl ApplyHooks for Live {
    fn snapshot(&self, generation: u64) -> Result<()> {
        let src = Self::sibling_live();
        let dest = Self::sibling_gen(generation);
        if src.is_dir() && self.catalog_root == Path::new("/oath") {
            let src_s = src.to_string_lossy().into_owned();
            let dest_s = dest.to_string_lossy().into_owned();
            if let Ok(st) = Command::new("btrfs")
                .args(["subvolume", "snapshot", "-r", &src_s, &dest_s])
                .status()
            {
                if st.success() {
                    tel(
                        "oath",
                        "snapshot",
                        json!({
                            "generation": generation,
                            "kind": "btrfs-sibling",
                            "path": dest_s
                        }),
                    );
                    return Ok(());
                }
            }
        }
        let dest = self.copy_fallback_dir(generation);
        fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))?;
        Self::copy_dir(&self.catalog_root, &dest)?;
        tel(
            "oath",
            "snapshot",
            json!({ "generation": generation, "kind": "copy", "path": dest.display().to_string() }),
        );
        Ok(())
    }

    fn restore_snapshot(&self, generation: u64) -> Result<()> {
        let sibling = Self::sibling_gen(generation).join("oath");
        if sibling.is_dir() {
            Self::restore_catalog(&sibling, Path::new("/oath"))?;
            tel(
                "oath",
                "restore",
                json!({ "generation": generation, "from": sibling.display().to_string() }),
            );
            return Ok(());
        }
        let dest = self.copy_fallback_dir(generation);
        let catalog_in_snap = dest.join("oath");
        if catalog_in_snap.is_dir() {
            Self::restore_catalog(&catalog_in_snap, Path::new("/oath"))?;
            tel(
                "oath",
                "restore",
                json!({ "generation": generation, "from": catalog_in_snap.display().to_string() }),
            );
            return Ok(());
        }
        if dest.is_dir() {
            Self::restore_catalog(&dest, &self.catalog_root)?;
            tel(
                "oath",
                "restore",
                json!({ "generation": generation, "from": dest.display().to_string() }),
            );
            return Ok(());
        }
        Err(Error::Msg(format!("no generation {generation} at {}", dest.display())))
    }

    fn converge_host(&self, desired: &Host) -> Result<Host> {
        sethostname(desired.hostname.as_str())
            .map_err(|e| Error::Msg(format!("sethostname: {e}")))?;
        oath_core::seat::write_side_effects(desired)?;
        tel("oath", "hostname", json!({ "name": desired.hostname }));
        Ok(Host {
            hostname: desired.hostname.clone(),
            power: HostPower::Run,
            env: desired.env.clone(),
            timezone: desired.timezone.clone(),
            session: desired.session,
        })
    }

    fn notify_init(&self) -> Result<()> {
        let sock = Path::new("/oath/run/init.sock");
        if !sock.exists() {
            return Ok(());
        }
        let mut s =
            UnixStream::connect(sock).map_err(|e| Error::Msg(format!("init socket: {e}")))?;
        s.write_all(b"converge\n").ok();
        Ok(())
    }

    fn wait_converge(&self, id: &ObjectId, enabled: bool) -> Result<()> {
        if id.kind != "svc" {
            return Ok(());
        }
        let want = if enabled { "running" } else { "stopped" };
        let path = Path::new("/oath/objects/svc").join(&id.name).join("actual.json");
        for _ in 0..40 {
            if let Ok(v) = oath_core::read_json::<serde_json::Value>(&path) {
                if v.get("state").and_then(|s| s.as_str()) == Some(want) {
                    return Ok(());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(Error::hint(format!("{id} did not become {want}"), format!("oath get {id}")))
    }

    fn reboot(&self) -> Result<()> {
        sync();
        tel("oath", "reboot", json!({}));
        reboot(RebootMode::RB_AUTOBOOT).map_err(|e| Error::Msg(format!("reboot: {e}")))?;
        Ok(())
    }

    fn halt(&self) -> Result<()> {
        sync();
        tel("oath", "halt", json!({}));
        // POWER_OFF, not HALT_SYSTEM: halt leaves QEMU running with stdio
        // attached (the host serial looks "stuck"). Power-off makes QEMU exit.
        reboot(RebootMode::RB_POWER_OFF).map_err(|e| Error::Msg(format!("halt: {e}")))?;
        Ok(())
    }

    fn converge_dev(&self, id: &ObjectId, desired: &Dev) -> Result<DevActual> {
        if self.catalog_root.as_path() != Path::new("/oath") {
            return Ok(DevActual {
                present: desired.present,
                class: String::new(),
                node: String::new(),
            });
        }
        let actual = converge_dev(id, desired)?;
        tel(
            "oath",
            "dev",
            json!({
                "id": id.to_string(),
                "present": actual.present,
                "class": actual.class,
                "node": actual.node,
            }),
        );
        Ok(actual)
    }

    fn converge_ssh(&self, id: &ObjectId, desired: &Ssh) -> Result<SshActual> {
        if self.catalog_root.as_path() != Path::new("/oath") {
            return Ok(SshActual { authorized: desired.authorized.clone(), host_key: false });
        }
        let actual = converge_ssh(desired)?;
        tel(
            "oath",
            "ssh",
            json!({
                "id": id.to_string(),
                "keys": actual.authorized.len(),
                "host_key": actual.host_key,
            }),
        );
        Ok(actual)
    }

    fn converge_net(&self, id: &ObjectId, desired: &Net) -> Result<Net> {
        if self.catalog_root.as_path() != Path::new("/oath") {
            return Ok(desired.clone());
        }
        let actual = converge_net(desired)?;
        tel(
            "oath",
            "net",
            json!({
                "id": id.to_string(),
                "up": actual.up,
                "ipv4": actual.ipv4,
                "gateway": actual.gateway,
            }),
        );
        Ok(actual)
    }

    fn converge_pkg(&self, id: &ObjectId, desired: &Pkg) -> Result<PkgActual> {
        let bin = if self.catalog_root.as_path() == Path::new("/oath") {
            PathBuf::from("/bin")
        } else {
            self.catalog_root.join("bin")
        };
        if desired.present && !desired.url.is_empty() {
            fetch_pkg(&self.catalog_root, &id.name, &desired.url, &desired.hash)?;
        }
        let actual =
            converge_pkg(&self.catalog_root, &bin, &id.name, desired.present, &desired.hash)?;
        tel(
            "oath",
            "pkg",
            json!({
                "id": id.to_string(),
                "present": actual.present,
                "links": actual.links,
            }),
        );
        Ok(actual)
    }
}

fn fetch_pkg(catalog_root: &Path, name: &str, url: &str, hash: &str) -> Result<()> {
    if store_present(catalog_root, name, hash) {
        return Ok(());
    }
    let url = fetch_url(url, name, hash);
    if url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/pkg").join(format!(".{name}.wget"));
    let _ = fs::remove_dir_all(&tmpdir);
    fs::create_dir_all(&tmpdir)?;
    let blob = tmpdir.join("blob");
    let st = Command::new("/bin/wget")
        .args(["-q", "-O", blob.to_str().unwrap(), &url])
        .status()
        .map_err(|e| Error::Msg(format!("wget: {e}")))?;
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::hint(format!("fetch pkg:{name} failed"), "oath schema pkg"));
    }
    let result = if looks_like_tar(&blob) {
        let tree = tmpdir.join("tree");
        fs::create_dir_all(&tree)?;
        extract_tar(&blob, &tree)?;
        install_tree(catalog_root, name, &tree, None, hash)
    } else {
        let bytes = fs::read(&blob)?;
        ingest_file(catalog_root, name, &bytes, None, hash)
    };
    let _ = fs::remove_dir_all(&tmpdir);
    result.map(|_| ())
}

fn looks_like_tar(path: &Path) -> bool {
    let Ok(mut fd) = fs::File::open(path) else {
        return false;
    };
    let mut hdr = [0u8; 262];
    let Ok(n) = fd.read(&mut hdr) else {
        return false;
    };
    if n >= 2 && hdr[0] == 0x1f && hdr[1] == 0x8b {
        return true;
    }
    n >= 262 && &hdr[257..262] == b"ustar"
}

fn extract_tar(blob: &Path, dest: &Path) -> Result<()> {
    let gzip = looks_like_gzip(blob);
    let mut cmd = Command::new("/bin/tar");
    cmd.arg("-C").arg(dest);
    if gzip {
        cmd.arg("-xzf");
    } else {
        cmd.arg("-xf");
    }
    cmd.arg(blob);
    let st = cmd.status().map_err(|e| Error::Msg(format!("tar: {e}")))?;
    if !st.success() {
        return Err(Error::hint("fetch pack tar extract failed", "oath schema pkg"));
    }
    Ok(())
}

fn looks_like_gzip(path: &Path) -> bool {
    let Ok(mut fd) = fs::File::open(path) else {
        return false;
    };
    let mut b = [0u8; 2];
    matches!(fd.read(&mut b), Ok(2) if b[0] == 0x1f && b[1] == 0x8b)
}
