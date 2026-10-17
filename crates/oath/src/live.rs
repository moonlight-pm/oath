use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::sys::reboot::{reboot, RebootMode};
use nix::unistd::{sethostname, sync};
use oath_core::{
    converge_dev, converge_net, converge_pkg, converge_ssh, fetch_adv, fetch_pkg, gen_subvol_name,
    tel, ApplyHooks, Dev, DevActual, Error, Host, HostPower, Net, ObjectId, Pkg, PkgActual, Result,
    Ssh, SshActual, BTRFS_TOP, LIVE_SUBVOL,
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

    /// Off btrfs (or `--root` elsewhere), a snapshot is a copy of
    /// `objects/` under `<root>/.gens/<n>`.
    fn copy_fallback_dir(&self, generation: u64) -> PathBuf {
        self.catalog_root.join(".gens").join(generation.to_string())
    }
}

/// Ask our PID 1 for a clean shutdown (it stops services, syncs and
/// remounts read-only first). Without it, sync and call the kernel.
fn power(sig: libc::c_int, fallback: RebootMode) -> Result<()> {
    if Path::new("/oath/run/init.sock").exists() && unsafe { libc::kill(1, sig) } == 0 {
        return Ok(());
    }
    sync();
    reboot(fallback).map_err(|e| Error::Msg(format!("reboot: {e}")))?;
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
        oath_core::copy_objects(&self.catalog_root, &dest)?;
        tel(
            "oath",
            "snapshot",
            json!({ "generation": generation, "kind": "copy", "path": dest.display().to_string() }),
        );
        Ok(())
    }

    fn snapshot_catalog(&self, generation: u64) -> Result<PathBuf> {
        let sibling = Self::sibling_gen(generation).join("oath");
        if self.catalog_root == Path::new("/oath") && sibling.join("objects").is_dir() {
            return Ok(sibling);
        }
        let copy = self.copy_fallback_dir(generation);
        if copy.join("objects").is_dir() {
            return Ok(copy);
        }
        Err(Error::Msg(format!("no snapshot of generation {generation}")))
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
        // PID 1 answers once it has converged, so apply returns after the
        // change has happened (sshd restarted, a service stopped).
        let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(60)));
        let mut reply = [0u8; 8];
        match std::io::Read::read(&mut s, &mut reply) {
            Ok(n) if n > 0 => Ok(()),
            _ => Err(Error::hint("PID 1 did not answer the converge request", "oath logs")),
        }
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
        tel("oath", "reboot", json!({}));
        power(libc::SIGTERM, RebootMode::RB_AUTOBOOT)
    }

    fn halt(&self) -> Result<()> {
        tel("oath", "halt", json!({}));
        // Power off, not halt: halt leaves QEMU (and a machine) sitting there.
        power(libc::SIGUSR2, RebootMode::RB_POWER_OFF)
    }

    fn converge_dev(&self, id: &ObjectId, desired: &Dev) -> Result<DevActual> {
        if self.catalog_root.as_path() != Path::new("/oath") {
            return Ok(DevActual { present: desired.present, ..Default::default() });
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
        let mut actual =
            converge_pkg(&self.catalog_root, &bin, &id.name, desired.present, &desired.hash)?;
        if !desired.url.is_empty() {
            let pin =
                if !actual.hash.is_empty() { actual.hash.as_str() } else { desired.hash.as_str() };
            let _ = fetch_adv(&self.catalog_root, &id.name, &desired.url, pin);
            actual.advisories = oath_core::read_advisories(&self.catalog_root, &id.name, pin);
        }
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
