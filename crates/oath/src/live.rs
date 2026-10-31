use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::sys::reboot::{reboot, RebootMode};
use nix::unistd::{sethostname, sync};
use oath_core::{
    converge_dev, converge_net, converge_pkg, converge_ssh, fetch_adv, fetch_pkg_any,
    gen_subvol_name, tel, ApplyHooks, Dev, DevActual, Error, Host, HostPower, Net, ObjectId, Pkg,
    PkgActual, Result, Ssh, SshActual, BTRFS_TOP, LIVE_SUBVOL,
};
use serde_json::json;

pub struct Live {
    pub catalog_root: PathBuf,
    pub follow: Follow,
}

/// What an apply asks of PID 1 once its packages have moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Follow {
    /// Converge: each running service whose package moved restarts on it.
    #[default]
    Now,
    /// An update's: converge, but a service with a `seat` keeps the
    /// program it runs until it next starts (`converge keep-seat`; a PID 1
    /// from before restarts it).
    KeepSeat,
    /// An update that reboots next: nothing; the boot starts every service
    /// on its new program.
    AtReboot,
}

impl Live {
    pub fn new(catalog_root: &Path) -> Self {
        Self { catalog_root: catalog_root.to_path_buf(), follow: Follow::Now }
    }

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
            // stdout carries only oath's own answer (`--json`).
            if let Ok(st) = Command::new("btrfs")
                .args(["subvolume", "snapshot", "-r", &src_s, &dest_s])
                .stdout(std::io::stderr())
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

    fn converge_host(&self, desired: &Host, previous: Option<&Host>) -> Result<Host> {
        sethostname(desired.hostname.as_str())
            .map_err(|e| Error::Msg(format!("sethostname: {e}")))?;
        oath_core::seat::write_side_effects(desired)?;
        tel("oath", "hostname", json!({ "name": desired.hostname }));
        // Mounts are this machine's: a catalog under --root is not.
        let mut mounts = Default::default();
        if self.catalog_root == Path::new("/oath") {
            let was = previous.map(|p| p.mounts.clone()).unwrap_or_default();
            let m = oath_core::mounts::converge(&desired.mounts, &was, false);
            for path in &m.waiting {
                eprintln!("host:local: {path} waits for {}", desired.mounts[path].source);
            }
            for path in &m.next_boot {
                eprintln!(
                    "host:local: {path} is {} from the next boot: PID 1 mounts it before sshd \
                     and the desk, and writes sshd's keys onto it",
                    desired.mounts[path].source
                );
            }
            if !(m.waiting.is_empty() && m.errors.is_empty() && was == desired.mounts) {
                let paths: Vec<_> = m.mounted.keys().collect();
                tel(
                    "oath",
                    "mounts",
                    json!({ "mounted": paths, "waiting": m.waiting, "errors": m.errors }),
                );
            }
            if !m.errors.is_empty() {
                return Err(Error::hint(
                    format!("host:local: {}", m.errors.join("; ")),
                    "oath get host:local; is the path in use (umount), or the option wrong?",
                ));
            }
            mounts = m.mounted;
        }
        Ok(Host {
            hostname: desired.hostname.clone(),
            power: HostPower::Run,
            env: desired.env.clone(),
            timezone: desired.timezone.clone(),
            hosts: desired.hosts.clone(),
            origins: desired.origins.clone(),
            profile: desired.profile.clone(),
            publishers: desired.publishers.clone(),
            mounts,
            module_options: desired.module_options.clone(),
            autologin: desired.autologin,
        })
    }

    fn notify_init(&self) -> Result<()> {
        let sock = Path::new("/oath/run/init.sock");
        let ask: &[u8] = match self.follow {
            Follow::Now => b"converge\n",
            Follow::KeepSeat => b"converge keep-seat\n",
            Follow::AtReboot => return Ok(()),
        };
        if !sock.exists() {
            return Ok(());
        }
        let mut s =
            UnixStream::connect(sock).map_err(|e| Error::Msg(format!("init socket: {e}")))?;
        s.write_all(ask).ok();
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
        // Before a reboot PID 1 is told nothing: the boot starts it (an
        // update that makes a service new to the profile, R47).
        if id.kind != "svc" || self.follow == Follow::AtReboot {
            return Ok(());
        }
        let want = if enabled { "running" } else { "stopped" };
        let path = oath_core::status_path(&self.catalog_root, id);
        let applied = self.catalog_root.join("objects/svc").join(&id.name).join("applied.json");
        let oneshot = oath_core::read_json::<serde_json::Value>(&applied)
            .is_ok_and(|s| s["restart"] == json!("never"));
        for _ in 0..40 {
            if let Ok(v) = oath_core::read_json::<serde_json::Value>(&path) {
                let state = v.get("state").and_then(|s| s.as_str());
                // A service the other owner of the seat runs (R42) is held;
                // a scheduled one (R48) waits for its time; a oneshot may
                // have run and exited already.
                if state == Some(want)
                    || (enabled && matches!(state, Some("held" | "scheduled")))
                    || (enabled && oneshot && state == Some("stopped") && !v["last_exit"].is_null())
                {
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
        let from = if desired.present {
            fetch_pkg_any(&self.catalog_root, &id.name, &desired.url, &desired.file, &desired.hash)?
        } else {
            None
        };
        let mut actual =
            converge_pkg(&self.catalog_root, &bin, &id.name, desired.present, &desired.hash)?;
        // Advisories come from the pkg's own url on every apply, and from an
        // origin when it has just served the pack. A vendor's file (`file`)
        // is not an origin: no advisories there.
        let adv =
            if !desired.url.is_empty() { Some(desired.url.as_str()) } else { from.as_deref() };
        if let Some(src) = adv.filter(|_| desired.file.is_empty()) {
            let pin =
                if !actual.hash.is_empty() { actual.hash.as_str() } else { desired.hash.as_str() };
            let _ = fetch_adv(&self.catalog_root, &id.name, src, pin);
            actual.advisories = oath_core::read_advisories(&self.catalog_root, &id.name, pin);
        }
        let mut ev = json!({
            "id": id.to_string(),
            "present": actual.present,
            "links": actual.links,
        });
        // The consumer records where a fetched pack came from; the pack does not.
        if let Some(src) = &from {
            ev["fetched"] = json!(oath_core::fetch_url(src, &id.name, &desired.hash));
        }
        tel("oath", "pkg", ev);
        Ok(actual)
    }
}
