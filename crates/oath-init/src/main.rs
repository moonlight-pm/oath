//! PID 1. Mounts, hostname from the catalog, supervises svc:*, reaps.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use nix::mount::{mount, MsFlags};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{sethostname, Pid};
use oath_core::{seed, tel, Catalog, Host, ObjectId, BTRFS_TOP, DEFAULT_ROOT};
use serde_json::json;

mod gpu;
mod hotplug;
mod modules;
mod probation;
mod resume;
mod shutdown;
mod supervise;

/// Lines logged before `/oath/log` exists (the initrd stage), written out
/// ahead of the first line that can be.
static EARLY: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());
const EARLY_MAX: usize = 64 * 1024;

/// To the console, and to `/oath/log/init.log` once the catalog is mounted
/// (`oath logs`).
pub(crate) fn log(msg: &str) {
    let _ = writeln!(std::io::stderr(), "oath-init: {msg}");
    let line = format!("{} {msg}\n", oath_core::now_rfc3339());
    let Ok(mut early) = EARLY.lock() else { return };
    if !Path::new(supervise::LOG_ROOT).is_dir() {
        if early.len() + line.len() <= EARLY_MAX {
            early.extend_from_slice(line.as_bytes());
        }
        return;
    }
    early.extend_from_slice(line.as_bytes());
    supervise::append_log("init", &std::mem::take(&mut *early));
}

fn kver() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string()
}

fn main() {
    // The kernel runs this binary as its modprobe helper too, the DHCP
    // client's hook as `net-lease`, and people as `reboot`, `halt` and
    // `poweroff`.
    let mut args = std::env::args();
    match args.next().as_deref().map(|a| a.rsplit('/').next().unwrap_or(a)) {
        Some("modprobe") => {
            std::process::exit(modules::modprobe_main(&args.collect::<Vec<_>>()));
        }
        Some(cmd @ ("reboot" | "halt" | "poweroff")) => {
            std::process::exit(shutdown::command_main(cmd, &args.collect::<Vec<_>>()));
        }
        Some("net-lease") => {
            let event = args.next().unwrap_or_default();
            let env = |k: &str| std::env::var(k).ok();
            if let Err(e) = oath_core::record_lease(Path::new(DEFAULT_ROOT), &event, &env) {
                eprintln!("net-lease: {e}");
            }
            // Never fail the client's script over a status file.
            std::process::exit(0);
        }
        _ => {}
    }
    // A panic in PID 1 panics the kernel; `panic=` on the command line
    // then reboots. Say why first.
    std::panic::set_hook(Box::new(|info| {
        log(&format!("panic: {info}"));
        tel("init", "panic", json!({ "msg": info.to_string() }));
    }));
    shutdown::install();
    if let Err(e) = real_main() {
        log(&format!("fatal: {e}"));
        tel("init", "fatal", json!({ "err": e }));
        fallback();
    }
}

fn real_main() -> Result<(), String> {
    if resume::resuming() {
        let r = resume::take()?;
        run(r.sup, r.listener, hotplug::Hotplug::from_socket(r.uevents), r.resumes);
    }
    tel("init", "boot", json!({ "pid": std::process::id(), "kver": kver() }));
    ensure_mount("proc", "/proc", "proc");
    ensure_mount("sysfs", "/sys", "sysfs");
    ensure_mount("devtmpfs", "/dev", "devtmpfs");
    // Filesystems and code pages load when the kernel asks for them.
    let _ = fs::write("/proc/sys/kernel/modprobe", modules::HELPER);
    // ICMP echo sockets for every group: `ping` works as home with no
    // setuid bit or capability.
    let _ = fs::write("/proc/sys/net/ipv4/ping_group_range", "0 2147483647");
    mount_devpts();
    unix_floor();

    if cmdline_flag("oath.install") {
        return install_ramdisk();
    }
    let hot = hotplug::Hotplug::open();

    if !Path::new("/oath/INDEX.md").exists() {
        load_modules(true);
        let dev = root_dev();
        let sub = root_subvol();
        mount_root(&dev, &sub)?;
        tel("init", "mounted", json!({ "dev": dev, "subvol": sub }));
    }

    let _ = fs::create_dir_all("/oath/run");
    let _ = fs::create_dir_all("/oath/log");
    let _ = fs::create_dir_all("/tmp");
    let _ = fs::create_dir_all("/root");
    mount_toplevel();
    mount_status();

    // Seed only a machine with no catalog at all. A missing INDEX.md is
    // regenerated, never a reason to throw the catalog away.
    if !Path::new("/oath/objects").is_dir() {
        let _ = seed(Path::new(DEFAULT_ROOT));
        log("seeded empty catalog");
        tel("init", "seeded", json!({}));
    }
    // Boot-time catalog writes hold the lock; svc status later does not.
    let boot_lock = Catalog::open(DEFAULT_ROOT).and_then(|c| c.lock());
    if let Err(e) = &boot_lock {
        log(&format!("catalog lock: {e}"));
    }
    // A catalog from before R14 kept status in its object directories.
    if let (Ok(cat), Ok(lock)) = (Catalog::open(DEFAULT_ROOT), &boot_lock) {
        match cat.migrate(lock) {
            Ok(0) => {}
            Ok(n) => log(&format!("catalog: {n} objects moved to applied.json and status/")),
            Err(e) => log(&format!("catalog migrate: {e}")),
        }
    }
    // The schema files describe the binaries on this machine, so an
    // upgraded oath brings its schemas with it.
    match oath_core::write_schemas(Path::new(DEFAULT_ROOT)) {
        Ok(files) if !files.is_empty() => log(&format!("schema: wrote {}", files.join(" "))),
        Ok(_) => {}
        Err(e) => log(&format!("schema: {e}")),
    }
    if !Path::new("/oath/INDEX.md").exists() {
        if let Ok(c) = Catalog::open(DEFAULT_ROOT) {
            let _ = c.write_index();
        }
    }

    apply_host();
    // Ethernet first. Do not insmod amdgpu before dhcp/sshd — on canto
    // there is no simpledrm, so a "defer KMS" pass that still required
    // firmware_fb_live() would load amdgpu and hang the NIC forever.
    load_modules(true);
    apply_net();
    // The inventory is what sysfs has (net0 is named by now).
    if let (Ok(cat), Ok(lock)) = (Catalog::open(DEFAULT_ROOT), &boot_lock) {
        hotplug::sync(&cat, lock);
    }
    ensure_home();
    inject_ssh_from_host();
    apply_ssh();
    // Packages, plans and the generation: observed from the store and /bin.
    if let (Ok(cat), Ok(lock)) = (Catalog::open(DEFAULT_ROOT), &boot_lock) {
        if let Err(e) = cat.observe(lock, Path::new("/bin")) {
            log(&format!("catalog observe: {e}"));
        }
    }
    drop(boot_lock);
    // Listen before the first service starts: an `oath apply` that comes
    // in over SSH while boot finishes waits in the backlog, not lost.
    let sock_path = "/oath/run/init.sock";
    let _ = fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path).map_err(|e| e.to_string())?;
    let _ = listener.set_nonblocking(true);
    let mut sup = supervise::Supervisor::new();
    // sshd and DHCP first, then the deferred KMS and sound modules.
    sup.converge();
    load_modules(false);

    log("ready");
    tel("init", "ready", json!({ "kver": kver() }));
    // The store's integrity, once a boot, off the boot path: every linked
    // package tree is hashed again against its pin.
    sup.run_task("verify", &["/bin/oath", "verify"]);
    run(sup, listener, hot, Vec::new())
}

/// The main loop. A panic in one pass does not take the machine down:
/// PID 1 hands its state to a fresh copy of itself (`resume`), and only
/// reboots when that keeps happening.
fn run(
    mut sup: supervise::Supervisor,
    listener: UnixListener,
    mut hot: hotplug::Hotplug,
    resumes: Vec<f64>,
) -> ! {
    let mut probation = probation::Watch::default();
    loop {
        let pass = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            step(&mut sup, &listener, &mut probation, &mut hot)
        }));
        if pass.is_err() {
            resume::handoff(&mut sup, listener, hot.take_socket(), resumes);
            shutdown::finish(&mut sup, shutdown::Mode::Reboot);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn step(
    sup: &mut supervise::Supervisor,
    listener: &UnixListener,
    probation: &mut probation::Watch,
    hot: &mut hotplug::Hotplug,
) {
    if let Some(mode) = shutdown::requested() {
        shutdown::finish(sup, mode);
    }
    sup.reap();
    sup.pump_logs();
    sup.start_due();
    probation.tick(sup);
    hot.tick();
    if let Ok((mut s, _)) = listener.accept() {
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf).unwrap_or(0);
        // The harness's way to prove a panic is survived; root only.
        if buf[..n].starts_with(b"panic") && peer_uid(&s) == Some(0) {
            panic!("asked to over init.sock");
        }
        tel("init", "converge", json!({ "bytes": n }));
        sup.converge();
        let _ = s.write_all(b"ok\n");
    }
}

fn peer_uid(s: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ok = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    } == 0;
    ok.then_some(cred.uid)
}

fn ensure_mount(fstype: &str, target: &str, source: &str) {
    if Path::new(target).join("self").exists() || Path::new(target).join("block").exists() {
        return;
    }
    let _ = fs::create_dir_all(target);
    let _ = mount(Some(source), target, Some(fstype), MsFlags::empty(), None::<&str>);
}

/// Dropbear allocates PTYs via `/dev/pts/ptmx`. Kernel default is
/// `mode=600,ptmxmode=000`, which refuses interactive SSH for `home`.
fn mount_devpts() {
    let _ = fs::create_dir_all("/dev/pts");
    let opts = "mode=0666,ptmxmode=0666";
    if !mounted("/dev/pts") {
        let _ = mount(Some("devpts"), "/dev/pts", Some("devpts"), MsFlags::empty(), Some(opts));
    }
    let _ = mount(Some("devpts"), "/dev/pts", Some("devpts"), MsFlags::MS_REMOUNT, Some(opts));
    open_dev_nodes();
}

/// The nodes every user needs (`/dev/null`, `/dev/urandom`, …) are 0666
/// and `/dev/tty1` is home's, whatever devtmpfs made them; this runs
/// after every pass.
fn open_dev_nodes() {
    for n in [
        "/dev/null",
        "/dev/zero",
        "/dev/full",
        "/dev/tty",
        "/dev/ptmx",
        "/dev/random",
        "/dev/urandom",
    ] {
        let _ = fs::set_permissions(n, fs::Permissions::from_mode(0o666));
    }
    // The screen's terminal is the seat's: `svc:screen` runs as home.
    let tty = "/dev/tty1";
    let _ = std::os::unix::fs::chown(tty, Some(oath_core::seat::UID), Some(oath_core::seat::GID));
    let _ = fs::set_permissions(tty, fs::Permissions::from_mode(0o600));
}

/// Coldplug by modalias; with `defer_kms`, display and sound wait.
fn load_modules(defer_kms: bool) {
    match modules::Index::open(&modules::base()) {
        Some(idx) => {
            let (loaded, failed) = idx.coldplug(defer_kms);
            if !loaded.is_empty() {
                log(&format!("modules: {}", loaded.join(" ")));
            }
            for e in &failed {
                log(&format!("module {e}"));
            }
            tel("init", "modules", json!({ "loaded": loaded, "failed": failed }));
        }
        None => log(&format!("no module index in {}", modules::base().display())),
    }
    open_dev_nodes();
}

fn cmdline() -> String {
    fs::read_to_string("/proc/cmdline").unwrap_or_default()
}

pub(crate) fn cmdline_flag(key: &str) -> bool {
    cmdline()
        .split_whitespace()
        .any(|t| t == key || t == format!("{key}=1") || t == format!("{key}=true"))
}

fn cmdline_val(key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    cmdline().split_whitespace().find_map(|t| t.strip_prefix(&prefix).map(|s| s.to_string()))
}

fn root_subvol() -> String {
    cmdline_val("oath.subvol")
        .and_then(|s| oath_core::boot_subvol(&s).map(|v| v.to_string()))
        .unwrap_or_else(|| oath_core::LIVE_SUBVOL.to_string())
}

fn root_dev() -> String {
    cmdline_val("oath.root").unwrap_or_else(|| {
        if Path::new("/dev/vda").exists() {
            "/dev/vda".into()
        } else if Path::new("/dev/sda2").exists() {
            "/dev/sda2".into()
        } else if Path::new("/dev/nvme0n1p2").exists() {
            "/dev/nvme0n1p2".into()
        } else {
            "/dev/vda".into()
        }
    })
}

fn install_ramdisk() -> Result<(), String> {
    load_modules(true);
    let _ = fs::create_dir_all("/root/.ssh");
    let _ = fs::create_dir_all("/etc");
    let _ = fs::create_dir_all("/var/run");
    let _ = fs::create_dir_all("/oath/log");
    let _ = fs::create_dir_all("/run/oath-install");
    if !Path::new("/etc/passwd").exists() {
        let _ = fs::write("/etc/passwd", "root:x:0:0:root:/root:/bin/sh\n");
    }
    let _ = fs::set_permissions("/root/.ssh", fs::Permissions::from_mode(0o700));
    start_install_tty0();
    bring_up_install_net();
    if !install_has_ipv4() {
        log("no ipv4 after dhcp; dump dmesg to ESP, keep ramdisk");
        dump_kexec_debug();
        bring_up_install_net();
    }
    start_install_dropbear();
    let _ = fs::write("/run/oath-install/ready", "1\n");
    log("install ramdisk ready (dropbear)");
    tel("init", "install", json!({ "ready": true }));
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn bring_up_install_net() {
    let nic = match pick_install_nic() {
        Some(n) => n,
        None => {
            log("no install nic");
            tel("init", "install-net", json!({ "ok": false, "err": "no nic" }));
            return;
        }
    };
    log(&format!("install nic {nic}"));
    timed_link_up(&nic, Duration::from_secs(2));
    if let Some(ip) = cmdline_val("oath.ip") {
        let st = Command::new("/bin/ip").args(["addr", "add", &ip, "dev", &nic]).status();
        log(&format!("ip addr add {ip} dev {nic} -> {st:?}"));
        if let Some(gw) = cmdline_val("oath.gw") {
            let st = Command::new("/bin/ip").args(["route", "add", "default", "via", &gw]).status();
            log(&format!("ip route default via {gw} -> {st:?}"));
        }
        tel("init", "install-net", json!({ "nic": nic, "ip": ip, "dhcp": false }));
        return;
    }
    for i in 0..5 {
        if nic_carrier(&nic) {
            break;
        }
        log(&format!("wait carrier {nic} {i}"));
        std::thread::sleep(Duration::from_millis(400));
    }
    for d in oath_core::DHCP_DIRS {
        let _ = fs::create_dir_all(d);
    }
    let st = Command::new("/bin/dhcpcd").args(oath_core::dhcpcd_oneshot(&nic, 16)).status();
    log(&format!("dhcpcd {nic} -> {st:?}"));
    tel("init", "install-net", json!({ "nic": nic, "dhcp": true }));
}

fn list_nics() -> Vec<String> {
    let mut v = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/class/net") else {
        return v;
    };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n == "lo" {
            continue;
        }
        v.push(n);
    }
    v.sort();
    v
}

fn nic_mac(n: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/net/{n}/address")).ok().map(|s| s.trim().to_lowercase())
}

fn nic_carrier(n: &str) -> bool {
    fs::read_to_string(format!("/sys/class/net/{n}/carrier"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

fn timed_link_up(nic: &str, timeout: Duration) {
    let mut child = match Command::new("/bin/ip").args(["link", "set", nic, "up"]).spawn() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("ip link set {nic} up spawn: {e}"));
            return;
        }
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                log(&format!("ip link set {nic} up -> {st}"));
                return;
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                log(&format!("ip link set {nic} up timed out"));
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                log(&format!("ip link set {nic} up wait: {e}"));
                return;
            }
        }
    }
}

fn pick_install_nic() -> Option<String> {
    let want_mac = cmdline_val("oath.mac").map(|s| s.to_lowercase());
    let want_name = cmdline_val("oath.nic");
    for i in 0..75 {
        let nics = list_nics();
        if i == 0 || i % 10 == 0 || !nics.is_empty() {
            let detail: Vec<String> = nics
                .iter()
                .map(|n| {
                    format!("{n} mac={} carrier={}", nic_mac(n).unwrap_or_default(), nic_carrier(n))
                })
                .collect();
            log(&format!("nics: {}", detail.join("; ")));
        }
        if let Some(mac) = &want_mac {
            for n in &nics {
                if nic_mac(n).as_deref() == Some(mac.as_str()) {
                    timed_link_up(n, Duration::from_secs(2));
                    return Some(n.clone());
                }
            }
        }
        for n in &nics {
            timed_link_up(n, Duration::from_secs(2));
            if nic_carrier(n) {
                return Some(n.clone());
            }
        }
        if let Some(want) = &want_name {
            if nics.iter().any(|n| n == want) {
                timed_link_up(want, Duration::from_secs(2));
                return Some(want.clone());
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

fn start_install_tty0() {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg("-l");
    cmd.env("PATH", "/bin").env("HOME", "/root").env("PS1", "oath-install# ");
    unsafe {
        cmd.pre_exec(|| {
            let fd = libc::open(c"/dev/tty0".as_ptr(), libc::O_RDWR);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            if fd > 2 {
                libc::close(fd);
            }
            libc::setsid();
            libc::ioctl(0, libc::TIOCSCTTY, 1);
            Ok(())
        });
    }
    match cmd.spawn() {
        Ok(_) => log("tty0 shell"),
        Err(e) => log(&format!("tty0 shell: {e}")),
    }
}

fn install_has_ipv4() -> bool {
    let o = Command::new("/bin/ip").arg("addr").output();
    let Ok(o) = o else {
        return false;
    };
    String::from_utf8_lossy(&o.stdout).lines().any(|l| {
        let t = l.trim();
        t.contains("inet ") && !t.contains("127.0.0.1") && !t.starts_with("inet6")
    })
}

fn dump_kexec_debug() {
    let mut body = String::new();
    body.push_str("cmdline: ");
    body.push_str(&cmdline());
    body.push('\n');
    body.push_str("nics: ");
    body.push_str(&list_nics().join(","));
    body.push('\n');
    if let Ok(o) = Command::new("/bin/ip").arg("addr").output() {
        body.push_str("ip addr:\n");
        body.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    if let Ok(rd) = fs::read_dir("/sys/bus/pci/devices") {
        body.push_str("pci:\n");
        let mut names: Vec<String> =
            rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        for n in names {
            let id =
                fs::read_to_string(format!("/sys/bus/pci/devices/{n}/uevent")).unwrap_or_default();
            body.push_str(&format!("{n} {id}"));
            if !body.ends_with('\n') {
                body.push('\n');
            }
        }
    }
    if let Ok(rd) = fs::read_dir("/lib/firmware/tigon") {
        body.push_str("firmware tigon: ");
        let mut n: Vec<_> =
            rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        n.sort();
        body.push_str(&n.join(","));
        body.push('\n');
    } else {
        body.push_str("firmware tigon: missing\n");
    }
    if let Ok(o) = Command::new("/bin/dmesg").output() {
        body.push_str("dmesg:\n");
        body.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    let _ = fs::create_dir_all("/esp");
    for cand in ["/dev/sda1", "/dev/nvme0n1p1", "/dev/vda1", "/dev/sdb1"] {
        if !Path::new(cand).exists() {
            continue;
        }
        let st = Command::new("/bin/mount").args(["-t", "vfat", cand, "/esp"]).status();
        if !matches!(st, Ok(s) if s.success()) {
            continue;
        }
        let _ = fs::write("/esp/oath-kexec.log", &body);
        let _ = Command::new("/bin/umount").arg("/esp").status();
        log(&format!("wrote /esp/oath-kexec.log via {cand}"));
        break;
    }
}

fn start_install_dropbear() {
    let _ = Command::new("/bin/dropbearkey")
        .args(["-t", "ed25519", "-f", "/run/oath-install/host_ed25519"])
        .status();
    let st = Command::new("/bin/dropbear")
        .args(["-E", "-s", "-D", "/root/.ssh", "-r", "/run/oath-install/host_ed25519", "-p", "22"])
        .status();
    log(&format!("dropbear -> {st:?}"));
}

fn mount_root(dev: &str, subvol: &str) -> Result<(), String> {
    let _ = fs::create_dir_all("/newroot");
    let flags = MsFlags::empty();
    let data = format!("subvol={subvol}");
    let first = mount(Some(dev), "/newroot", Some("btrfs"), flags, Some(data.as_str()));
    if first.is_err() && subvol != oath_core::LIVE_SUBVOL {
        log(&format!("subvol {subvol} failed, falling back to @"));
        mount(Some(dev), "/newroot", Some("btrfs"), flags, Some("subvol=@"))
            .map_err(|e| format!("mount root {dev}: {e}"))?;
    } else {
        first.map_err(|e| format!("mount root {dev} {subvol}: {e}"))?;
    }
    keep_initrd_mods("/newroot");
    // switch_root: move the API filesystems into the disk root, move the
    // disk root over `/`, and chroot into it, so `/` really is the disk.
    // The initramfs stays in memory: its modules and firmware are bind
    // mounted into the new root for the deferred KMS pass.
    for m in ["/dev", "/proc", "/sys", "/run"] {
        let target = format!("/newroot{m}");
        let _ = fs::create_dir_all(&target);
        if let Err(e) =
            mount(Some(m), target.as_str(), None::<&str>, MsFlags::MS_MOVE, None::<&str>)
        {
            log(&format!("move {m}: {e}"));
        }
    }
    std::env::set_current_dir("/newroot").map_err(|e| e.to_string())?;
    mount(Some("."), "/", None::<&str>, MsFlags::MS_MOVE, None::<&str>)
        .map_err(|e| format!("move root to /: {e}"))?;
    nix::unistd::chroot(".").map_err(|e| format!("chroot: {e}"))?;
    std::env::set_current_dir("/").ok();
    ensure_mount("proc", "/proc", "proc");
    ensure_mount("sysfs", "/sys", "sysfs");
    ensure_mount("devtmpfs", "/dev", "devtmpfs");
    mount_devpts();
    unix_floor();
    Ok(())
}

/// Keep initrd modules/firmware after chroot so the deferred KMS pass can load them.
fn keep_initrd_mods(newroot: &str) {
    for rel in ["lib/modules", "lib/firmware"] {
        let src = Path::new("/").join(rel);
        let dst = Path::new(newroot).join(rel);
        if !src.is_dir() {
            continue;
        }
        let _ = fs::create_dir_all(&dst);
        let _ =
            mount(Some(src.as_path()), dst.as_path(), None::<&str>, MsFlags::MS_BIND, None::<&str>);
    }
}

/// Is `target` a mount point already (after switch_root, /dev/shm and
/// /run came along from the initramfs)?
fn mounted(target: &str) -> bool {
    fs::read_to_string("/proc/self/mountinfo")
        .unwrap_or_default()
        .lines()
        .any(|l| l.split_whitespace().nth(4) == Some(target))
}

fn unix_floor() {
    let flags = MsFlags::MS_NOSUID | MsFlags::MS_NODEV;
    for (dir, mode) in [("/tmp", "mode=1777"), ("/dev/shm", "mode=1777"), ("/run", "mode=755")] {
        let _ = fs::create_dir_all(dir);
        if !mounted(dir) {
            let _ = mount(Some("tmpfs"), dir, Some("tmpfs"), flags, Some(mode));
        }
    }
    let _ = fs::create_dir_all("/run/user/0");
    let _ = fs::set_permissions("/run/user/0", std::fs::Permissions::from_mode(0o700));
    let xdg = format!("/run/user/{}", oath_core::seat::UID);
    let _ = fs::create_dir_all(&xdg);
    let _ = fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700));
    // Shell process substitution opens /dev/fd/N.
    let _ = std::os::unix::fs::symlink("/proc/self/fd", "/dev/fd");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/0", "/dev/stdin");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/1", "/dev/stdout");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/2", "/dev/stderr");
    if !Path::new("/etc/machine-id").is_file() {
        let _ = fs::create_dir_all("/etc");
        let _ = fs::write("/etc/machine-id", "00000000000000000000000000000001\n");
    }
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    let _ =
        mount(Some("cgroup2"), "/sys/fs/cgroup", Some("cgroup2"), MsFlags::empty(), None::<&str>);
    // net:net0 is the NIC; lo is not a catalog object.
    let _ = Command::new("/bin/ip").args(["link", "set", "lo", "up"]).status();
    let _ = Command::new("/bin/ip").args(["addr", "add", "127.0.0.1/8", "dev", "lo"]).status();
    let _ = Command::new("/bin/ip").args(["addr", "add", "::1/128", "dev", "lo"]).status();
}

/// `id`'s status file (on the `/oath/status` tmpfs).
fn status(id: &ObjectId) -> std::path::PathBuf {
    oath_core::status_path(Path::new(DEFAULT_ROOT), id)
}

/// Status lives on a tmpfs at `/oath/status` (R14): no generation or boot
/// snapshot holds it, and every boot observes afresh. Whatever an older
/// `oath` left in the directory underneath is cleared first, so it is not
/// carried in snapshots either.
fn mount_status() {
    let dir = Path::new(DEFAULT_ROOT).join(oath_core::STATUS_DIR);
    if mounted(&dir.to_string_lossy()) {
        return;
    }
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::create_dir_all(&dir);
    let flags = MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC;
    if let Err(e) = mount(Some("tmpfs"), &dir, Some("tmpfs"), flags, Some("mode=755")) {
        log(&format!("status tmpfs: {e}; status stays on disk this boot"));
    }
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
    let dev = root_dev();
    let err = mount(Some(dev.as_str()), top, Some("btrfs"), MsFlags::empty(), Some(data));
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

/// What was last applied (`applied.json`). A catalog from before
/// applied.json kept it in `actual`; one that has never been applied
/// starts from `desired`. Pending `oath set` edits do not take effect
/// until `oath apply`.
pub(crate) fn applied<T: serde::de::DeserializeOwned>(
    cat: &Catalog,
    obj: &oath_core::Object,
) -> Option<T> {
    cat.applied(&obj.id)
        .ok()
        .flatten()
        .and_then(|a| serde_json::from_value(a).ok())
        .or_else(|| serde_json::from_value(obj.actual.clone()).ok())
        .or_else(|| serde_json::from_value(obj.desired.clone()).ok())
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
    let Some(host) = applied::<Host>(&cat, &obj) else { return };
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
    let _ = oath_core::seat::write_side_effects(&host);
    let actual = Host {
        hostname: host.hostname,
        power: oath_core::HostPower::Run,
        env: host.env,
        timezone: host.timezone,
    };
    let _ = oath_core::write_json(&status(&id), &actual);
    let _ = cat.write_index();
}

fn apply_net() {
    let cat = match Catalog::open(DEFAULT_ROOT) {
        Ok(c) => c,
        Err(_) => return,
    };
    let id = ObjectId::new("net", "net0");
    let Ok(obj) = cat.get(&id) else { return };
    let Some(net) = applied::<oath_core::Net>(&cat, &obj) else { return };
    match oath_core::converge_net(&net) {
        Ok(actual) => {
            tel("init", "net", json!({ "ok": true, "up": actual.up, "ipv4": actual.ipv4 }));
            let _ = oath_core::write_json(&status(&id), &actual);
        }
        Err(e) => {
            log(&format!("net: {e}"));
            tel("init", "net", json!({ "ok": false, "err": e.to_string() }));
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
    // Boot applies ssh from its applied spec, so the keys go in both.
    let dir = Path::new(DEFAULT_ROOT).join("objects/ssh/local");
    let _ = oath_core::write_json(&dir.join("desired.json"), &ssh);
    let _ = oath_core::write_json(&dir.join("applied.json"), &ssh);
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
    let Some(ssh) = applied::<oath_core::Ssh>(&cat, &obj) else { return };
    match oath_core::converge_ssh(&ssh) {
        Ok(actual) => {
            tel("init", "ssh", json!({ "ok": true, "host_key": actual.host_key }));
            let _ = oath_core::write_json(&status(&id), &actual);
        }
        Err(e) => {
            log(&format!("ssh: {e}"));
            tel("init", "ssh", json!({ "ok": false, "err": e.to_string() }));
        }
    }
}

/// `/home` and `/home/.ssh` exist and belong to `home`.
fn ensure_home() {
    use oath_core::seat;
    let home = Path::new(seat::HOME);
    let ssh = home.join(".ssh");
    let _ = fs::create_dir_all(&ssh);
    let own = format!("{}:{}", seat::UID, seat::GID);
    let _ = Command::new("/bin/chown").args(["-R", &own, seat::HOME]).status();
    let _ = Command::new("/bin/chmod").args(["755", seat::HOME]).status();
    let _ = Command::new("/bin/chmod").args(["700", &ssh.to_string_lossy()]).status();
}

/// Boot failed: a root shell on the console, again whenever it exits,
/// reaping every child so nothing piles up as a zombie.
fn fallback() {
    loop {
        log("dropping to /bin/sh");
        let shell = Command::new("/bin/sh").spawn();
        let Ok(child) = shell else {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        };
        let pid = Pid::from_raw(child.id() as i32);
        loop {
            match waitpid(Pid::from_raw(-1), None) {
                Ok(WaitStatus::Exited(p, _)) | Ok(WaitStatus::Signaled(p, _, _)) if p == pid => {
                    break
                }
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_secs(1)),
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
