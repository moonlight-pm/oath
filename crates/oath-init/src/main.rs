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
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{sethostname, Pid};
use oath_core::{seed, tel, Catalog, Host, ObjectId, Svc, BTRFS_TOP, DEFAULT_ROOT};
use serde_json::json;

const MODULES: &[&str] = &[
    "kernel/drivers/virtio/virtio.ko",
    "kernel/drivers/virtio/virtio_ring.ko",
    "kernel/drivers/virtio/virtio_pci_legacy_dev.ko",
    "kernel/drivers/virtio/virtio_pci_modern_dev.ko",
    "kernel/drivers/virtio/virtio_pci.ko",
    "kernel/drivers/block/virtio_blk.ko",
    "kernel/drivers/virtio/virtio_dma_buf.ko",
    "kernel/drivers/gpu/drm/virtio/virtio-gpu.ko",
    "kernel/drivers/virtio/virtio_input.ko",
    "kernel/net/core/failover.ko",
    "kernel/drivers/net/net_failover.ko",
    "kernel/drivers/net/virtio_net.ko",
    "kernel/drivers/char/hw_random/rng-core.ko",
    "kernel/drivers/char/hw_random/virtio-rng.ko",
    "kernel/drivers/firmware/qemu_fw_cfg.ko",
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
    let _ = fs::create_dir_all("/dev/pts");
    ensure_mount("devpts", "/dev/pts", "devpts");
    unix_floor();

    if !Path::new("/oath/INDEX.md").exists() {
        load_modules();
        mount_root()?;
        tel("init", "mounted", json!({ "dev": "/dev/vda", "subvol": "@" }));
    }

    let _ = fs::create_dir_all("/oath/run");
    let _ = fs::create_dir_all("/oath/log");
    let _ = fs::create_dir_all("/tmp");
    let _ = fs::create_dir_all("/root");
    mount_toplevel();

    if !Path::new("/oath/INDEX.md").exists() {
        let _ = seed(Path::new(DEFAULT_ROOT));
        log("seeded empty catalog");
        tel("init", "seeded", json!({}));
    }

    apply_host();
    apply_net();
    apply_dev();
    banner_tty0();
    inject_ssh_from_host();
    apply_ssh();
    let mut kids: HashMap<i32, Kid> = HashMap::new();
    converge(&mut kids);

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
            converge(&mut kids);
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
        if !ok {
            tel("init", "module", json!({ "name": m, "ok": false }));
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
    let _ = fs::create_dir_all("/dev/pts");
    ensure_mount("devpts", "/dev/pts", "devpts");
    unix_floor();
    Ok(())
}

fn unix_floor() {
    let flags = MsFlags::MS_NOSUID | MsFlags::MS_NODEV;
    let _ = fs::create_dir_all("/tmp");
    let _ = mount(Some("tmpfs"), "/tmp", Some("tmpfs"), flags, Some("mode=1777"));
    let _ = fs::create_dir_all("/dev/shm");
    let _ = mount(Some("tmpfs"), "/dev/shm", Some("tmpfs"), flags, Some("mode=1777"));
    let _ = fs::create_dir_all("/run");
    let _ = mount(Some("tmpfs"), "/run", Some("tmpfs"), flags, Some("mode=755"));
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    let _ =
        mount(Some("cgroup2"), "/sys/fs/cgroup", Some("cgroup2"), MsFlags::empty(), None::<&str>);
}

/// Mount btrfs subvolid=0 so generations can be sibling `@gen-N`, not nested on `@`.
fn mount_toplevel() {
    let top = Path::new(BTRFS_TOP);
    let _ = fs::create_dir_all(top);
    if top.join("@").is_dir() {
        tel("init", "fs_top", json!({ "path": BTRFS_TOP, "ok": true, "already": true }));
        return;
    }
    let data = "subvolid=0";
    let err = mount(Some("/dev/vda"), top, Some("btrfs"), MsFlags::empty(), Some(data));
    let ok = top.join("@").is_dir();
    tel(
        "init",
        "fs_top",
        json!({
            "path": BTRFS_TOP,
            "ok": ok,
            "err": err.err().map(|e| e.to_string()),
        }),
    );
    if !ok {
        log("btrfs top-level mount failed; snapshots will copy");
    }
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
    let _ = fs::write(
        "/etc/hosts",
        format!("127.0.0.1 localhost\n::1 localhost\n127.0.1.1 {}\n", host.hostname),
    );
    let actual = Host { hostname: host.hostname, power: oath_core::HostPower::Run };
    let dir = Path::new(DEFAULT_ROOT).join("objects/host/local");
    let _ = oath_core::write_json(&dir.join("actual.json"), &actual);
    let _ = cat.write_index();
}

fn apply_net() {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let id = ObjectId::new("net", "net0");
    let Ok(obj) = cat.get(&id) else { return };
    let Ok(net) = serde_json::from_value::<oath_core::Net>(obj.desired) else { return };
    match oath_core::converge_net(&net) {
        Ok(actual) => {
            tel("init", "net", json!({ "ok": true, "up": actual.up, "ipv4": actual.ipv4 }));
            let dir = Path::new(DEFAULT_ROOT).join("objects/net/net0");
            let _ = oath_core::write_json(&dir.join("actual.json"), &actual);
        }
        Err(e) => {
            log(&format!("net: {e}"));
            tel("init", "net", json!({ "ok": false, "err": e.to_string() }));
        }
    }
}

fn banner_tty0() {
    if let Ok(mut f) = fs::OpenOptions::new().write(true).open("/dev/tty0") {
        let _ = writeln!(f, "Oath.");
    }
}

fn apply_dev() {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let Ok(ids) = cat.ls(Some("dev")) else { return };
    for id in ids {
        let Ok(obj) = cat.get(&id) else { continue };
        let Ok(dev) = serde_json::from_value::<oath_core::Dev>(obj.desired) else { continue };
        match oath_core::converge_dev(&id, &dev) {
            Ok(actual) => {
                tel(
                    "init",
                    "dev",
                    json!({ "id": id.to_string(), "present": actual.present, "node": actual.node }),
                );
                let dir = Path::new(DEFAULT_ROOT).join("objects/dev").join(&id.name);
                let _ = oath_core::write_json(&dir.join("actual.json"), &actual);
            }
            Err(e) => log(&format!("dev {id}: {e}")),
        }
    }
}

fn inject_ssh_from_host() {
    let raw = Path::new("/sys/firmware/qemu_fw_cfg/by_name/opt/oath/authorized/raw");
    let Ok(body) = fs::read_to_string(raw) else {
        return;
    };
    let extra: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("ssh-") || l.starts_with("ecdsa-"))
        .map(|s| s.to_string())
        .collect();
    if extra.is_empty() {
        return;
    }
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let id = ObjectId::new("ssh", "local");
    let Ok(obj) = cat.get(&id) else { return };
    let Ok(mut ssh) = serde_json::from_value::<oath_core::Ssh>(obj.desired) else { return };
    let mut n = 0usize;
    for k in extra {
        let blob = k.split_whitespace().nth(1).unwrap_or("");
        if ssh.authorized.iter().any(|e| e.split_whitespace().nth(1) == Some(blob)) {
            continue;
        }
        ssh.authorized.push(k);
        n += 1;
    }
    if n == 0 {
        return;
    }
    let dir = Path::new(DEFAULT_ROOT).join("objects/ssh/local");
    let _ = oath_core::write_json(&dir.join("desired.json"), &ssh);
    tel("init", "ssh_inject", json!({ "added": n, "total": ssh.authorized.len() }));
    log(&format!("injected {n} host SSH public key(s)"));
}

fn apply_ssh() {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let id = ObjectId::new("ssh", "local");
    let Ok(obj) = cat.get(&id) else { return };
    let Ok(ssh) = serde_json::from_value::<oath_core::Ssh>(obj.desired) else { return };
    match oath_core::converge_ssh(&ssh) {
        Ok(actual) => {
            tel("init", "ssh", json!({ "ok": true, "host_key": actual.host_key }));
            let dir = Path::new(DEFAULT_ROOT).join("objects/ssh/local");
            let _ = oath_core::write_json(&dir.join("actual.json"), &actual);
        }
        Err(e) => {
            log(&format!("ssh: {e}"));
            tel("init", "ssh", json!({ "ok": false, "err": e.to_string() }));
        }
    }
}

struct Kid {
    id: String,
    spec: Svc,
}

fn pid_for(kids: &HashMap<i32, Kid>, id: &str) -> Option<i32> {
    kids.iter().find(|(_, k)| k.id == id).map(|(p, _)| *p)
}

fn stop_kid(kids: &mut HashMap<i32, Kid>, id: &str) {
    if let Some(pid) = pid_for(kids, id) {
        if let Some(k) = kids.remove(&pid) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
            tel("init", "svc_stop", json!({ "id": k.id, "pid": pid }));
            let oid: ObjectId = k.id.parse().unwrap_or_else(|_| ObjectId::new("svc", "x"));
            write_svc_actual(&oid, "stopped", None, 0);
        }
    }
}

fn converge(kids: &mut HashMap<i32, Kid>) {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let Ok(ids) = cat.ls(Some("svc")) else { return };
    let mut wanted: HashMap<String, Svc> = HashMap::new();
    for id in &ids {
        if let Ok(obj) = cat.get(id) {
            if let Ok(spec) = serde_json::from_value::<Svc>(obj.desired) {
                wanted.insert(id.to_string(), spec);
            }
        }
    }

    let running: Vec<(i32, String, Svc)> =
        kids.iter().map(|(p, k)| (*p, k.id.clone(), k.spec.clone())).collect();
    for (_pid, id, spec) in running {
        let keep = wanted
            .get(&id)
            .map(|s| s.enabled && !s.exec.is_empty() && s.exec == spec.exec)
            .unwrap_or(false);
        if !keep {
            stop_kid(kids, &id);
        }
    }

    let items: Vec<(String, Svc)> = wanted.iter().map(|(id, s)| (id.clone(), s.clone())).collect();
    let order = match oath_core::svc_start_order(&items) {
        Ok(o) => o,
        Err(e) => {
            log(&format!("svc wants: {e}"));
            tel("init", "svc_wants", json!({ "ok": false, "err": e.to_string() }));
            Vec::new()
        }
    };
    for id in &order {
        let Some(spec) = wanted.get(id) else { continue };
        let oid: ObjectId = id.parse().unwrap_or_else(|_| ObjectId::new("svc", "x"));
        if !spec.enabled || spec.exec.is_empty() {
            if pid_for(kids, id).is_none() {
                write_svc_actual(&oid, "stopped", None, 0);
            }
            continue;
        }
        if pid_for(kids, id).is_some() {
            continue;
        }
        match spawn(spec) {
            Ok(pid) => {
                tel("init", "svc_start", json!({ "id": id, "pid": pid.as_raw() }));
                write_svc_actual(&oid, "running", Some(pid.as_raw()), 0);
                kids.insert(pid.as_raw(), Kid { id: id.clone(), spec: spec.clone() });
            }
            Err(e) => {
                log(&format!("{id} spawn: {e}"));
                tel("init", "svc_fail", json!({ "id": id, "err": e }));
                write_svc_actual(&oid, "failed", None, 0);
            }
        }
    }
    for (id, spec) in &wanted {
        if spec.enabled {
            continue;
        }
        let oid: ObjectId = id.parse().unwrap_or_else(|_| ObjectId::new("svc", "x"));
        if pid_for(kids, id).is_none() {
            write_svc_actual(&oid, "stopped", None, 0);
        }
    }
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
