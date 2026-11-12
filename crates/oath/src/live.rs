use std::fs;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use nix::sys::reboot::{reboot, RebootMode};
use nix::unistd::{sethostname, sync};
use oath_core::{
    converge_dev, converge_net, converge_ssh, fetch_adv, fetch_and_converge_pkg,
    gen_subvol_name, parse_gen_subvol, tel, ApplyHooks, Dev, DevActual, Error, Host, HostPower, Net, ObjectId, Pkg,
    PkgActual, Result, Slept, Ssh, SshActual, BTRFS_TOP, LIVE_SUBVOL,
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

    /// Every generation kept for undo: its number, and the catalog root in
    /// its snapshot.
    pub fn generations(&self) -> Vec<(u64, PathBuf)> {
        let mut out = Vec::new();
        if self.catalog_root == Path::new("/oath") {
            for e in fs::read_dir(BTRFS_TOP).into_iter().flatten().flatten() {
                if let Some(n) = parse_gen_subvol(&e.file_name().to_string_lossy()) {
                    out.push((n, e.path().join("oath")));
                }
            }
        }
        for e in fs::read_dir(self.catalog_root.join(".gens")).into_iter().flatten().flatten() {
            if let Ok(n) = e.file_name().to_string_lossy().parse() {
                out.push((n, e.path()));
            }
        }
        out.sort_by_key(|(n, _)| *n);
        out
    }

    /// Take a generation off the disk: `oath undo` cannot go back to it.
    pub fn remove_generation(&self, generation: u64) -> Result<()> {
        let copy = self.copy_fallback_dir(generation);
        if copy.is_dir() {
            fs::remove_dir_all(&copy)?;
            return Ok(());
        }
        let sub = Self::sibling_gen(generation);
        let out = Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(&sub)
            .stdout(Stdio::null())
            .output()
            .map_err(|e| Error::Msg(format!("btrfs: {e}")))?;
        if !out.status.success() {
            return Err(Error::Msg(format!(
                "btrfs subvolume delete {}: {}",
                sub.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(())
    }
}

/// What PID 1 says of itself (R73), written at boot and by each image that
/// takes over; a PID 1 from before writes none.
const INIT_SELF: &str = "/run/oath/init.json";

/// What came of asking PID 1 to re-execute into a new binary (R73).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reexec {
    /// PID 1 runs the new binary, with every service as it was.
    Done,
    /// This PID 1 predates re-executing on request: the new one runs from
    /// the next boot. It is not asked, since it takes any message it does
    /// not know for a converge.
    Unsupported,
}

/// Write all of `buf` to PID 1's socket. A PID 1 that closed it is an
/// error here: `oath` leaves SIGPIPE at its default, which would end it
/// part way through an apply.
pub fn send(s: &UnixStream, mut buf: &[u8]) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    while !buf.is_empty() {
        let n = unsafe {
            libc::send(s.as_raw_fd(), buf.as_ptr().cast(), buf.len(), libc::MSG_NOSIGNAL)
        };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        buf = &buf[n as usize..];
    }
    Ok(())
}

/// Whether PID 1 re-executes into a new binary on request (R73).
pub fn can_reexec() -> bool {
    oath_core::read_json::<serde_json::Value>(Path::new(INIT_SELF))
        .is_ok_and(|v| v["upgrade"].as_u64().unwrap_or(0) >= 1)
}

/// The hash of the binary PID 1 runs, as root.
pub fn init_hash() -> Result<String> {
    Ok(oath_core::file_hash(&fs::read("/proc/1/exe")?))
}

/// Ask PID 1 to re-execute into `bin`, whose hash is `hash`, and wait until
/// the new binary has taken over (R73). PID 1 hashes the file itself, has
/// it read a copy of its state, and execs the file it hashed; services keep
/// running and stay its children. An error says why it did not, and PID 1
/// then runs what it ran.
pub fn reexec_init(bin: &Path, hash: &str) -> Result<Reexec> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let me = Path::new(INIT_SELF);
    let says = |p: &Path| oath_core::read_json::<serde_json::Value>(p).ok();
    let Some(before) = says(me).filter(|v| v["upgrade"].as_u64().unwrap_or(0) >= 1) else {
        return Ok(Reexec::Unsupported);
    };
    let ino = fs::metadata(me)?.ino();
    let mut s = UnixStream::connect("/oath/run/init.sock")
        .map_err(|e| Error::Msg(format!("init socket: {e}")))?;
    send(&s, format!("upgrade {} {hash}\n", bin.display()).as_bytes())?;
    // PID 1 closes the connection by executing the new binary, or says why
    // it did not.
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(60)));
    let mut reply = String::new();
    let _ = s.read_to_string(&mut reply);
    if let Some(why) = reply.lines().find_map(|l| l.strip_prefix("refused: ")) {
        return Err(Error::hint(format!("PID 1 did not re-execute: {why}"), "oath logs"));
    }
    if !reply.starts_with("upgrading") {
        return Err(Error::hint(
            format!("PID 1 did not take the request (it said {:?})", reply.trim()),
            "oath logs",
        ));
    }
    for _ in 0..300 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if fs::metadata(me).map(|m| m.ino()).ok() == Some(ino) {
            continue;
        }
        let Some(now) = says(me) else { continue };
        if now["exe"] == json!(hash) {
            return Ok(Reexec::Done);
        }
        if now["exe"] == before["exe"] {
            return Err(Error::hint(
                "the new binary could not take PID 1's state; PID 1 went back to the one it ran, \
                 with every service",
                "oath logs",
            ));
        }
    }
    Err(Error::hint(
        format!("PID 1 did not say within 30 s that it runs {hash}"),
        "oath logs; `/run/oath/init.json` says what PID 1 runs",
    ))
}

/// Whether a service whose status PID 1 wrote as `v` is what an apply
/// waits for, `restart` being its applied policy. A service the other owner
/// of the seat runs (R42) is held; a scheduled one (R48) waits for its
/// time. One that need not stay up may have run and exited already: a
/// `never` one however it exited, an `on-failure` one with exit 0, which
/// PID 1 keeps down. An `on-failure` one that failed is not running.
fn converged(v: &serde_json::Value, restart: &serde_json::Value, enabled: bool) -> bool {
    let state = v["state"].as_str();
    let finished = match restart.as_str() {
        Some("never") => !v["last_exit"].is_null(),
        Some("on-failure") => v["last_exit"] == json!({ "code": 0 }),
        _ => false,
    };
    if !enabled {
        return state == Some("stopped");
    }
    let up = matches!(state, Some("running" | "held" | "scheduled"));
    up || (state == Some("stopped") && finished)
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
        Err(Error::hint(
            format!("no snapshot of generation {generation}: it is past what undo can reach"),
            format!(
                "a machine keeps its newest {} generations and those since it booted (R62)",
                oath_core::reap::GENERATIONS_KEPT
            ),
        ))
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
            // The seat's HID interfaces (R71) change now, not at the next plug.
            if previous.map_or(&[][..], |p| &p.seat_hid[..]) != desired.seat_hid {
                let nodes = oath_core::hid::open_nodes(
                    Path::new("/sys"),
                    Path::new("/dev"),
                    &desired.seat_hid,
                );
                if nodes.is_empty() && !desired.seat_hid.is_empty() {
                    eprintln!("host:local: seat_hid names no vendor interface plugged in now");
                }
                for n in &nodes {
                    eprintln!("host:local: /dev/{n} is the seat's (root:home 0660)");
                }
                tel("oath", "seat_hid", json!({ "ids": desired.seat_hid, "nodes": nodes }));
            }
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
            seat_hid: desired.seat_hid.clone(),
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
        send(&s, ask).ok();
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
        let restart = oath_core::read_json::<serde_json::Value>(&applied)
            .map(|s| s["restart"].clone())
            .unwrap_or_default();
        for _ in 0..40 {
            if let Ok(v) = oath_core::read_json::<serde_json::Value>(&path) {
                if converged(&v, &restart, enabled) {
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

    /// No PID 1 step: the kernel freezes every process, PID 1 too, and
    /// thaws them on wake. A catalog other than /oath does not sleep the
    /// machine.
    fn sleep(&self) -> Result<Slept> {
        if self.catalog_root.as_path() != Path::new("/oath") {
            return Ok(Slept::default());
        }
        crate::sleep::sleep()
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
        // The tree is hashed once: the fetch's hash of it serves the link.
        let (mut actual, from) =
            fetch_and_converge_pkg(&self.catalog_root, &bin, &id.name, desired)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_that_finished_as_its_policy_allows_has_converged() {
        let stopped = |exit: serde_json::Value| json!({ "state": "stopped", "last_exit": exit });
        let (ok, failed) = (json!({ "code": 0 }), json!({ "code": 1 }));
        let killed = json!({ "signal": 9 });
        let (never, on_failure, always) = (json!("never"), json!("on-failure"), json!("always"));
        assert!(converged(&json!({ "state": "running" }), &always, true));
        assert!(converged(&json!({ "state": "held" }), &always, true));
        assert!(converged(&stopped(ok.clone()), &never, true));
        assert!(converged(&stopped(failed.clone()), &never, true));
        assert!(converged(&stopped(ok.clone()), &on_failure, true));
        assert!(!converged(&stopped(failed), &on_failure, true), "a failed run is not running");
        assert!(!converged(&stopped(killed), &on_failure, true));
        assert!(!converged(&stopped(ok.clone()), &always, true), "PID 1 starts it again");
        assert!(!converged(&json!({ "state": "stopped" }), &on_failure, true), "it never ran");
        assert!(converged(&stopped(ok), &on_failure, false));
        assert!(!converged(&json!({ "state": "running" }), &never, false));
    }
}
