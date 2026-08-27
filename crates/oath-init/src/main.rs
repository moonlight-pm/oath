//! PID 1. Mounts, hostname from the catalog, supervises svc:*, reaps.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use nix::mount::{mount, MsFlags};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{sethostname, Pid};
use oath_core::{seed, tel, Catalog, Host, ObjectId, Svc, DEFAULT_ROOT};
use serde_json::json;

const MODULES: &[&str] = &[
    "kernel/drivers/virtio/virtio.ko",
    "kernel/drivers/virtio/virtio_ring.ko",
    "kernel/drivers/virtio/virtio_pci_legacy_dev.ko",
    "kernel/drivers/virtio/virtio_pci_modern_dev.ko",
    "kernel/drivers/virtio/virtio_pci.ko",
    "kernel/drivers/block/virtio_blk.ko",
    "kernel/crypto/crc32c_generic.ko",
    "kernel/lib/libcrc32c.ko",
    "kernel/crypto/xor.ko",
    "kernel/lib/raid6/raid6_pq.ko",
    "kernel/fs/btrfs/btrfs.ko",
];

fn log(msg: &str) {
    let _ = writeln!(std::io::stderr(), "oath-init: {msg}");
}

fn kver() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string()
}

fn main() {
    if let Err(e) = real_main() {
        log(&format!("fatal: {e}"));
        tel("init", "fatal", json!({ "err": e }));
        fallback();
    }
}

fn real_main() -> Result<(), String> {
    tel("init", "boot", json!({ "pid": std::process::id(), "kver": kver() }));
    ensure_mount("proc", "/proc", "proc");
    ensure_mount("sysfs", "/sys", "sysfs");
    ensure_mount("devtmpfs", "/dev", "devtmpfs");

    if !Path::new("/oath/INDEX.md").exists() {
        load_modules();
        mount_root()?;
        tel("init", "mounted", json!({ "dev": "/dev/vda", "subvol": "@" }));
    }

    let _ = fs::create_dir_all("/oath/run");
    let _ = fs::create_dir_all("/oath/log");
    let _ = fs::create_dir_all("/.oath-gens");
    let _ = fs::create_dir_all("/tmp");
    let _ = fs::create_dir_all("/root");

    if !Path::new("/oath/INDEX.md").exists() {
        let _ = seed(Path::new(DEFAULT_ROOT));
        log("seeded empty catalog");
        tel("init", "seeded", json!({}));
    }

    apply_host();
    let mut kids = start_services();

    let sock_path = "/oath/run/init.sock";
    let _ = fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path).map_err(|e| e.to_string())?;
    let _ = listener.set_nonblocking(true);

    log("ready");
    tel("init", "ready", json!({ "svcs": kids.len(), "kver": kver() }));
    loop {
        reap(&mut kids);
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            let n = s.read(&mut buf).unwrap_or(0);
            tel("init", "converge", json!({ "bytes": n }));
            kids = start_services();
            let _ = s.write_all(b"ok\n");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn ensure_mount(fstype: &str, target: &str, source: &str) {
    if Path::new(target).join("self").exists() || Path::new(target).join("block").exists() {
        return;
    }
    let _ = fs::create_dir_all(target);
    let _ = mount(Some(source), target, Some(fstype), MsFlags::empty(), None::<&str>);
}

fn load_modules() {
    let rel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let rel = rel.trim();
    let base = Path::new("/lib/modules").join(rel);
    for m in MODULES {
        let p = base.join(m);
        if !p.exists() {
            log(&format!("no module {m}"));
            tel("init", "module", json!({ "name": m, "ok": false, "err": "missing" }));
            continue;
        }
        let st = Command::new("/bin/busybox").args(["insmod", p.to_str().unwrap()]).status();
        let ok = matches!(st, Ok(s) if s.success());
        tel("init", "module", json!({ "name": m, "ok": ok }));
        if !ok {
            log(&format!("insmod {m} -> {st:?}"));
        }
    }
}

fn mount_root() -> Result<(), String> {
    let _ = fs::create_dir_all("/newroot");
    let flags = MsFlags::empty();
    mount(Some("/dev/vda"), "/newroot", Some("btrfs"), flags, Some("subvol=@"))
        .map_err(|e| format!("mount root: {e}"))?;
    // Switch into the disk. Keep this process as PID 1.
    std::env::set_current_dir("/newroot").map_err(|e| e.to_string())?;
    nix::unistd::chroot("/newroot").map_err(|e| format!("chroot: {e}"))?;
    std::env::set_current_dir("/").ok();
    ensure_mount("proc", "/proc", "proc");
    ensure_mount("sysfs", "/sys", "sysfs");
    ensure_mount("devtmpfs", "/dev", "devtmpfs");
    Ok(())
}

fn apply_host() {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(e) => {
            log(&format!("catalog: {e}"));
            return;
        }
    };
    let id = ObjectId::new("host", "local");
    let Ok(obj) = cat.get(&id) else { return };
    let Ok(host) = serde_json::from_value::<Host>(obj.desired) else { return };
    if sethostname(host.hostname.as_str()).is_err() {
        log("sethostname failed");
        tel("init", "hostname", json!({ "ok": false, "name": host.hostname }));
    } else {
        tel("init", "hostname", json!({ "ok": true, "name": host.hostname }));
    }
    let _ = fs::create_dir_all("/etc");
    let _ = fs::write("/etc/hostname", format!("{}\n", host.hostname));
    let actual = Host { hostname: host.hostname, power: oath_core::HostPower::Run };
    let dir = Path::new(DEFAULT_ROOT).join("objects/host/local");
    let _ = oath_core::write_json(&dir.join("actual.json"), &actual);
    let _ = cat.write_index();
}

struct Kid {
    id: String,
    spec: Svc,
}

fn start_services() -> HashMap<i32, Kid> {
    let mut kids = HashMap::new();
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return kids,
    };
    let Ok(ids) = cat.ls(Some("svc")) else { return kids };
    for id in ids {
        let Ok(obj) = cat.get(&id) else { continue };
        let Ok(spec) = serde_json::from_value::<Svc>(obj.desired) else { continue };
        if !spec.enabled || spec.exec.is_empty() {
            write_svc_actual(&id, "stopped", None, 0);
            continue;
        }
        match spawn(&spec) {
            Ok(pid) => {
                log(&format!("{} pid {}", id, pid.as_raw()));
                tel("init", "svc_start", json!({ "id": id.to_string(), "pid": pid.as_raw() }));
                write_svc_actual(&id, "running", Some(pid.as_raw()), 0);
                kids.insert(pid.as_raw(), Kid { id: id.to_string(), spec });
            }
            Err(e) => {
                log(&format!("{id} spawn: {e}"));
                tel("init", "svc_fail", json!({ "id": id.to_string(), "err": e }));
                write_svc_actual(&id, "failed", None, 0);
            }
        }
    }
    kids
}

fn spawn(spec: &Svc) -> Result<Pid, String> {
    let mut cmd = Command::new(&spec.exec[0]);
    if spec.exec.len() > 1 {
        cmd.args(&spec.exec[1..]);
    }
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    unsafe {
        cmd.pre_exec(|| {
            // new session; serial-login takes the tty
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    Ok(Pid::from_raw(child.id() as i32))
}

fn reap(kids: &mut HashMap<i32, Kid>) {
    loop {
        let (pid, failed) = match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(pid, code)) => (pid, code != 0),
            Ok(WaitStatus::Signaled(pid, _, _)) => (pid, true),
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        };
        let raw = pid.as_raw();
        let Some(k) = kids.remove(&raw) else { continue };
        log(&format!("{} reaped", k.id));
        tel("init", "svc_reap", json!({ "id": k.id, "failed": failed }));
        let restart = match k.spec.restart {
            oath_core::SvcRestart::Never => false,
            oath_core::SvcRestart::Always => true,
            oath_core::SvcRestart::OnFailure => failed,
        };
        let id: ObjectId = k.id.parse().unwrap_or_else(|_| ObjectId::new("svc", "x"));
        if restart {
            std::thread::sleep(Duration::from_millis(200));
            if let Ok(npid) = spawn(&k.spec) {
                write_svc_actual(&id, "running", Some(npid.as_raw()), 1);
                kids.insert(npid.as_raw(), Kid { id: k.id, spec: k.spec });
            }
        } else {
            write_svc_actual(&id, "stopped", None, 0);
        }
    }
}

fn write_svc_actual(id: &ObjectId, state: &str, pid: Option<i32>, restarts: u32) {
    let dir = Path::new(DEFAULT_ROOT).join("objects").join(&id.kind).join(&id.name);
    let v = serde_json::json!({ "state": state, "pid": pid, "restarts": restarts });
    let _ = oath_core::write_json(&dir.join("actual.json"), &v);
}

fn fallback() {
    log("dropping to /bin/sh");
    let _ = Command::new("/bin/sh").status();
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
