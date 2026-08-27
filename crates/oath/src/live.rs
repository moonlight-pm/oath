use std::fs;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::sys::reboot::{reboot, RebootMode};
use nix::unistd::{sethostname, sync};
use oath_core::{tel, ApplyHooks, Error, Host, HostPower, Result};
use serde_json::json;

pub struct Live {
    pub catalog_root: PathBuf,
}

impl Live {
    fn gens_dir(&self) -> PathBuf {
        if self.catalog_root == Path::new("/oath") {
            PathBuf::from("/.oath-gens")
        } else {
            self.catalog_root.join(".gens")
        }
    }

    fn on_btrfs_root() -> bool {
        #[cfg(target_os = "linux")]
        {
            const BTRFS: u64 = 0x9123_683E;
            if let Ok(s) = nix::sys::statfs::statfs("/") {
                return s.filesystem_type().0 == BTRFS;
            }
        }
        false
    }

    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        if dst.exists() {
            let _ = fs::remove_dir_all(dst);
        }
        copy_recursive(src, dst)
    }
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        let from = e.path();
        if from.is_dir() {
            copy_recursive(&from, &to)?;
        } else if from.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

impl ApplyHooks for Live {
    fn snapshot(&self, generation: u64) -> Result<()> {
        let gens = self.gens_dir();
        fs::create_dir_all(&gens)?;
        let dest = gens.join(generation.to_string());
        if Self::on_btrfs_root() && self.catalog_root == Path::new("/oath") {
            let dest_s = dest.to_string_lossy().into_owned();
            if let Ok(st) =
                Command::new("btrfs").args(["subvolume", "snapshot", "-r", "/", &dest_s]).status()
            {
                if st.success() {
                    tel(
                        "oath",
                        "snapshot",
                        json!({ "generation": generation, "kind": "btrfs", "path": dest_s }),
                    );
                    return Ok(());
                }
            }
        }
        Self::copy_dir(&self.catalog_root, &dest)?;
        tel(
            "oath",
            "snapshot",
            json!({ "generation": generation, "kind": "copy", "path": dest.display().to_string() }),
        );
        Ok(())
    }

    fn restore_snapshot(&self, generation: u64) -> Result<()> {
        let dest = self.gens_dir().join(generation.to_string());
        let catalog_in_snap = dest.join("oath");
        if catalog_in_snap.is_dir() {
            Self::copy_dir(&catalog_in_snap, Path::new("/oath"))?;
            tel(
                "oath",
                "restore",
                json!({ "generation": generation, "from": catalog_in_snap.display().to_string() }),
            );
            return Ok(());
        }
        if dest.is_dir() {
            Self::copy_dir(&dest, &self.catalog_root)?;
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
        let _ = fs::create_dir_all("/etc");
        let _ = fs::write("/etc/hostname", format!("{}\n", desired.hostname));
        tel("oath", "hostname", json!({ "name": desired.hostname }));
        Ok(Host { hostname: desired.hostname.clone(), power: HostPower::Run })
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

    fn reboot(&self) -> Result<()> {
        sync();
        tel("oath", "reboot", json!({}));
        reboot(RebootMode::RB_AUTOBOOT).map_err(|e| Error::Msg(format!("reboot: {e}")))?;
        Ok(())
    }

    fn halt(&self) -> Result<()> {
        sync();
        tel("oath", "halt", json!({}));
        reboot(RebootMode::RB_HALT_SYSTEM).map_err(|e| Error::Msg(format!("halt: {e}")))?;
        Ok(())
    }
}
