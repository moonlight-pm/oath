//! PID 1. Mounts, hostname from the catalog, supervises svc:*, reaps.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::mount::{mount, MsFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{sethostname, Pid};
use oath_core::{seed, tel, Catalog, Host, ObjectId, Svc, BTRFS_TOP, DEFAULT_ROOT};
use serde_json::json;

mod gpu;
#[allow(dead_code)]
mod splash;

/// Fallback if packed `load-order` is missing. Deps before users.
const MODULES: &[&str] = &[
    "kernel/drivers/virtio/virtio.ko",
    "kernel/drivers/virtio/virtio_ring.ko",
    "kernel/drivers/virtio/virtio_pci_legacy_dev.ko",
    "kernel/drivers/virtio/virtio_pci_modern_dev.ko",
    "kernel/drivers/virtio/virtio_pci.ko",
    "kernel/drivers/block/virtio_blk.ko",
    "kernel/drivers/virtio/virtio_dma_buf.ko",
    "kernel/drivers/gpu/drm/virtio/virtio-gpu.ko",
    "kernel/drivers/input/evdev.ko",
    "kernel/drivers/virtio/virtio_input.ko",
    "kernel/net/core/failover.ko",
    "kernel/drivers/net/net_failover.ko",
    "kernel/drivers/net/virtio_net.ko",
    "kernel/net/packet/af_packet.ko",
    "kernel/drivers/char/hw_random/rng-core.ko",
    "kernel/drivers/char/hw_random/virtio-rng.ko",
    "kernel/drivers/firmware/qemu_fw_cfg.ko",
    "kernel/crypto/crc32c_generic.ko",
    "kernel/lib/libcrc32c.ko",
    "kernel/crypto/xor.ko",
    "kernel/lib/raid6/raid6_pq.ko",
    "kernel/fs/btrfs/btrfs.ko",
    "kernel/fs/fat/fat.ko",
    "kernel/fs/fat/vfat.ko",
    "kernel/fs/nls/nls_cp437.ko",
    "kernel/fs/nls/nls_iso8859-1.ko",
    "kernel/fs/nls/nls_utf8.ko",
    "kernel/drivers/scsi/scsi_common.ko",
    "kernel/drivers/scsi/scsi_mod.ko",
    "kernel/drivers/scsi/sd_mod.ko",
    "kernel/drivers/ata/libata.ko",
    "kernel/drivers/ata/libahci.ko",
    "kernel/drivers/ata/ahci.ko",
    "kernel/drivers/nvme/common/nvme-auth.ko",
    "kernel/drivers/nvme/host/nvme-core.ko",
    "kernel/drivers/nvme/host/nvme.ko",
    "kernel/drivers/leds/led-class.ko",
    "kernel/drivers/pps/pps_core.ko",
    "kernel/drivers/ptp/ptp.ko",
    "kernel/drivers/dca/dca.ko",
    "kernel/drivers/i2c/algos/i2c-algo-bit.ko",
    "kernel/drivers/net/phy/libphy.ko",
    "kernel/drivers/net/mdio/fwnode_mdio.ko",
    "kernel/drivers/net/phy/fixed_phy.ko",
    "kernel/drivers/net/mdio/of_mdio.ko",
    "kernel/drivers/net/phy/mdio_devres.ko",
    "kernel/drivers/net/ethernet/broadcom/tg3.ko",
    "kernel/drivers/net/ethernet/intel/e1000e/e1000e.ko",
    "kernel/drivers/net/ethernet/intel/igb/igb.ko",
    "kernel/drivers/net/ethernet/realtek/r8169.ko",
    "kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko",
    "kernel/drivers/usb/host/xhci-hcd.ko",
    "kernel/drivers/usb/host/xhci-pci.ko",
    "kernel/drivers/usb/host/ehci-hcd.ko",
    "kernel/drivers/usb/host/ehci-pci.ko",
    "kernel/drivers/hid/hid.ko",
    "kernel/drivers/hid/hid-generic.ko",
    "kernel/drivers/hid/hid-apple.ko",
    "kernel/drivers/hid/usbhid/usbhid.ko",
    "kernel/sound/pci/hda/snd-hda-intel.ko",
    "kernel/sound/pci/hda/snd-hda-codec-hdmi.ko",
    "kernel/sound/pci/hda/snd-hda-codec-cirrus.ko",
    "kernel/sound/pci/hda/snd-hda-codec-generic.ko",
    "kernel/sound/usb/snd-usb-audio.ko",
    "kernel/sound/virtio/virtio_snd.ko",
    "kernel/net/sunrpc/sunrpc.ko",
    "kernel/fs/nfs_common/grace.ko",
    "kernel/fs/netfs/netfs.ko",
    "kernel/fs/nfs_common/nfs_localio.ko",
    "kernel/fs/lockd/lockd.ko",
    "kernel/fs/nfs/nfs.ko",
    "kernel/net/dns_resolver/dns_resolver.ko",
    "kernel/fs/nfs/nfsv4.ko",
];

pub(crate) fn log(msg: &str) {
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
    mount_devpts();
    unix_floor();

    if cmdline_flag("oath.install") {
        return install_ramdisk();
    }

    if !Path::new("/oath/INDEX.md").exists() {
        load_modules(true);
        let dev = root_dev();
        mount_root(&dev)?;
        tel("init", "mounted", json!({ "dev": dev, "subvol": "@" }));
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
    // Ethernet first. Do not insmod amdgpu before dhcp/sshd — on canto
    // there is no simpledrm, so a "defer KMS" pass that still required
    // firmware_fb_live() would load amdgpu and hang the NIC forever.
    load_modules(true);
    apply_net();
    apply_dev();
    ensure_seat();
    inject_ssh_from_host();
    apply_ssh();
    hold_graphics();
    let mut kids: HashMap<i32, Kid> = HashMap::new();
    let mut oneshot_done: HashSet<String> = HashSet::new();
    // sshd/dhcp first. River as `home` on amdgpu crash-loops if it starts
    // before KMS and /dev/input exist (libinput ENOENT, WLR on simpledrm).
    converge(&mut kids, false, &mut oneshot_done);
    load_modules(false);
    wait_seat_devices(Duration::from_secs(10));
    converge(&mut kids, true, &mut oneshot_done);

    let sock_path = "/oath/run/init.sock";
    let _ = fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path).map_err(|e| e.to_string())?;
    let _ = listener.set_nonblocking(true);

    log("ready");
    tel("init", "ready", json!({ "svcs": kids.len(), "kver": kver() }));
    let mut last_dev = Instant::now();
    loop {
        reap(&mut kids, &mut oneshot_done);
        if last_dev.elapsed() >= Duration::from_secs(2) {
            oath_core::seat::open_device_nodes();
            last_dev = Instant::now();
        }
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            let n = s.read(&mut buf).unwrap_or(0);
            tel("init", "converge", json!({ "bytes": n }));
            converge(&mut kids, true, &mut oneshot_done);
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

/// Dropbear allocates PTYs via `/dev/pts/ptmx`. Kernel default is
/// `mode=600,ptmxmode=000`, which refuses interactive SSH for `home`.
fn mount_devpts() {
    let _ = fs::create_dir_all("/dev/pts");
    let opts = "mode=0666,ptmxmode=0666";
    let _ = mount(Some("devpts"), "/dev/pts", Some("devpts"), MsFlags::empty(), Some(opts));
    let _ = mount(Some("devpts"), "/dev/pts", Some("devpts"), MsFlags::MS_REMOUNT, Some(opts));
    let _ = fs::set_permissions("/dev/ptmx", fs::Permissions::from_mode(0o666));
    for n in ["/dev/null", "/dev/zero", "/dev/tty", "/dev/random", "/dev/urandom"] {
        let _ = fs::set_permissions(n, fs::Permissions::from_mode(0o666));
    }
}

fn load_modules(defer_kms: bool) {
    let defer = defer_kms;
    let rel = kver();
    let base = Path::new("/lib/modules").join(&rel);
    let list = module_load_order(&base);
    for m in list {
        if defer && load_late(&m) {
            continue;
        }
        if module_already_loaded(&m) {
            continue;
        }
        let p = base.join(&m);
        if !p.exists() {
            log(&format!("no module {m}"));
            tel("init", "module", json!({ "name": m, "ok": false, "err": "missing" }));
            continue;
        }
        let mut cmd = Command::new("/bin/busybox");
        cmd.arg("insmod").arg(p.to_str().unwrap());
        if m.ends_with("amdgpu.ko") {
            cmd.args(["si_support=1", "cik_support=1"]);
        }
        let st = cmd.status();
        let ok = matches!(st, Ok(s) if s.success());
        if !ok {
            tel("init", "module", json!({ "name": m, "ok": false }));
            log(&format!("insmod {m} -> {st:?}"));
        }
    }
    let _ = Command::new("/bin/busybox").args(["mdev", "-s"]).status();
    if !defer {
        wait_drm(Duration::from_secs(5));
    }
}

/// KMS takeover plus ALSA. HDMI audio sits on the GPU function; load
/// snd after amdgpu so the HDA controller is there. Analog Intel HDA
/// can wait the same two seconds.
fn load_late(rel: &str) -> bool {
    gpu::takes_over_firmware_fb(rel) || rel.contains("/sound/")
}

fn module_already_loaded(rel: &str) -> bool {
    let stem = Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let name = stem.replace('-', "_");
    Path::new("/sys/module").join(name).exists()
}

fn wait_drm(wait: Duration) {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if drm_has_connected() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_seat_devices(wait: Duration) {
    let _ = fs::create_dir_all("/dev/input");
    let _ = fs::create_dir_all("/dev/dri");
    let deadline = Instant::now() + wait;
    loop {
        oath_core::seat::open_device_nodes();
        let drm = oath_core::seat::drm_nodes_ready();
        let input = oath_core::seat::input_nodes_ready();
        if drm && input {
            log("seat devices ready");
            tel("init", "seat_devices", json!({ "drm": drm, "input": input, "ok": true }));
            return;
        }
        if Instant::now() >= deadline {
            log(&format!("seat devices timeout drm={drm} input={input}"));
            tel("init", "seat_devices", json!({ "drm": drm, "input": input, "ok": false }));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn drm_has_connected() -> bool {
    let Ok(cards) = fs::read_dir("/sys/class/drm") else {
        return false;
    };
    for e in cards.flatten() {
        let p = e.path();
        let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if !name.contains('-') {
            continue;
        }
        if fs::read_to_string(p.join("status")).ok().is_some_and(|s| s.trim() == "connected") {
            return true;
        }
    }
    false
}

fn hold_graphics() {
    let _ = fs::write("/proc/sys/kernel/printk", "0 0 0 0\n");
    let _ = fs::write("/sys/class/graphics/fbcon/cursor_blink", b"0");
    if let Ok(vt) = fs::read_dir("/sys/class/vtconsole") {
        for e in vt.flatten() {
            let _ = fs::write(e.path().join("bind"), b"0");
        }
    }
    unsafe {
        let fd = libc::open(c"/dev/tty0".as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::ioctl(fd, 0x4B3A as libc::Ioctl, 1);
            libc::close(fd);
        }
        let fd = libc::open(c"/dev/tty1".as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::ioctl(fd, 0x4B3A as libc::Ioctl, 1);
            libc::close(fd);
        }
    }
}

fn module_load_order(base: &Path) -> Vec<String> {
    let p = base.join("load-order");
    if let Ok(s) = fs::read_to_string(&p) {
        let v: Vec<String> =
            s.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
        if !v.is_empty() {
            return v;
        }
    }
    MODULES.iter().map(|s| (*s).to_string()).collect()
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
        let st =
            Command::new("/bin/busybox").args(["ip", "addr", "add", &ip, "dev", &nic]).status();
        log(&format!("ip addr add {ip} dev {nic} -> {st:?}"));
        if let Some(gw) = cmdline_val("oath.gw") {
            let st = Command::new("/bin/busybox")
                .args(["ip", "route", "add", "default", "via", &gw])
                .status();
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
    let st = Command::new("/bin/busybox")
        .args([
            "udhcpc",
            "-i",
            &nic,
            "-n",
            "-q",
            "-f",
            "-s",
            "/lib/oath/udhcpc.script",
            "-T",
            "2",
            "-t",
            "8",
        ])
        .status();
    log(&format!("udhcpc {nic} -> {st:?}"));
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
    let mut child =
        match Command::new("/bin/busybox").args(["ip", "link", "set", nic, "up"]).spawn() {
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
    let mut cmd = Command::new("/bin/busybox");
    cmd.args(["sh", "-l"]);
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
    let o = Command::new("/bin/busybox").args(["ip", "addr"]).output();
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
    if let Ok(o) = Command::new("/bin/busybox").args(["ip", "addr"]).output() {
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
    if let Ok(o) = Command::new("/bin/busybox").arg("dmesg").output() {
        body.push_str("dmesg:\n");
        body.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    let _ = fs::create_dir_all("/esp");
    for cand in ["/dev/sda1", "/dev/nvme0n1p1", "/dev/vda1", "/dev/sdb1"] {
        if !Path::new(cand).exists() {
            continue;
        }
        let st = Command::new("/bin/busybox").args(["mount", "-t", "vfat", cand, "/esp"]).status();
        if !matches!(st, Ok(s) if s.success()) {
            continue;
        }
        let _ = fs::write("/esp/oath-kexec.log", &body);
        let _ = Command::new("/bin/busybox").args(["umount", "/esp"]).status();
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

fn mount_root(dev: &str) -> Result<(), String> {
    let _ = fs::create_dir_all("/newroot");
    let flags = MsFlags::empty();
    mount(Some(dev), "/newroot", Some("btrfs"), flags, Some("subvol=@"))
        .map_err(|e| format!("mount root {dev}: {e}"))?;
    keep_initrd_mods("/newroot");
    // Switch into the disk. Keep this process as PID 1.
    std::env::set_current_dir("/newroot").map_err(|e| e.to_string())?;
    nix::unistd::chroot("/newroot").map_err(|e| format!("chroot: {e}"))?;
    std::env::set_current_dir("/").ok();
    ensure_mount("proc", "/proc", "proc");
    ensure_mount("sysfs", "/sys", "sysfs");
    ensure_mount("devtmpfs", "/dev", "devtmpfs");
    mount_devpts();
    unix_floor();
    Ok(())
}

/// Keep initrd modules/firmware after chroot so amdgpu can load just before River.
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

fn unix_floor() {
    let flags = MsFlags::MS_NOSUID | MsFlags::MS_NODEV;
    let _ = fs::create_dir_all("/tmp");
    let _ = mount(Some("tmpfs"), "/tmp", Some("tmpfs"), flags, Some("mode=1777"));
    let _ = fs::create_dir_all("/dev/shm");
    let _ = mount(Some("tmpfs"), "/dev/shm", Some("tmpfs"), flags, Some("mode=1777"));
    let _ = fs::create_dir_all("/run");
    let _ = mount(Some("tmpfs"), "/run", Some("tmpfs"), flags, Some("mode=755"));
    let _ = fs::create_dir_all("/run/user/0");
    let _ = fs::set_permissions("/run/user/0", std::fs::Permissions::from_mode(0o700));
    let xdg = format!("/run/user/{}", oath_core::seat::UID);
    let _ = fs::create_dir_all(&xdg);
    let _ = fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700));
    // glib/dbus look here; we do not ship dbus-daemon.
    if !Path::new("/etc/machine-id").is_file() {
        let _ = fs::create_dir_all("/etc");
        let _ = fs::write("/etc/machine-id", "00000000000000000000000000000001\n");
    }
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    let _ =
        mount(Some("cgroup2"), "/sys/fs/cgroup", Some("cgroup2"), MsFlags::empty(), None::<&str>);
    // Steam/CEF bind 127.0.0.1. net:net0 is the NIC; lo is not a catalog object.
    let _ = Command::new("/bin/ip").args(["link", "set", "lo", "up"]).status();
    let _ = Command::new("/bin/ip").args(["addr", "add", "127.0.0.1/8", "dev", "lo"]).status();
    let _ = Command::new("/bin/ip").args(["addr", "add", "::1/128", "dev", "lo"]).status();
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
    let _ = oath_core::seat::write_side_effects(&host);
    let actual = Host {
        hostname: host.hostname,
        power: oath_core::HostPower::Run,
        env: host.env,
        timezone: host.timezone,
    };
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

fn converge(kids: &mut HashMap<i32, Kid>, start_seat: bool, oneshot_done: &mut HashSet<String>) {
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
        if !start_seat && oath_core::seat::is_seat_svc(id) {
            continue;
        }
        if !spec.enabled || spec.exec.is_empty() {
            oneshot_done.remove(id);
            if pid_for(kids, id).is_none() {
                write_svc_actual(&oid, "stopped", None, 0);
            }
            continue;
        }
        if pid_for(kids, id).is_some() {
            continue;
        }
        if spec.restart == oath_core::SvcRestart::Never && oneshot_done.contains(id) {
            continue;
        }
        match spawn(id, spec) {
            Ok(pid) => {
                tel("init", "svc_start", json!({ "id": id, "pid": pid.as_raw() }));
                write_svc_actual(&oid, "running", Some(pid.as_raw()), 0);
                if spec.restart == oath_core::SvcRestart::Never {
                    oneshot_done.insert(id.clone());
                }
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

fn host_timezone() -> Option<String> {
    let cat = Catalog::open(DEFAULT_ROOT).ok()?;
    let id = ObjectId::new("host", "local");
    let obj = cat.get(&id).ok()?;
    let host: Host = serde_json::from_value(obj.desired).ok()?;
    let tz = host.timezone.trim();
    if tz.is_empty() {
        None
    } else {
        Some(tz.to_string())
    }
}

fn host_env() -> Vec<(String, String)> {
    let cat = Catalog::open(DEFAULT_ROOT).ok();
    let Some(cat) = cat else { return Vec::new() };
    let id = ObjectId::new("host", "local");
    let Ok(obj) = cat.get(&id) else { return Vec::new() };
    let Ok(host) = serde_json::from_value::<Host>(obj.desired) else {
        return Vec::new();
    };
    host.env
        .into_iter()
        .filter(|(k, v)| oath_core::seat::valid_env_name(k) && !v.contains('\n'))
        .collect()
}

fn ensure_seat() {
    use oath_core::seat;
    let home = Path::new(seat::HOME);
    let ssh = home.join(".ssh");
    let xdg = format!("/run/user/{}", seat::UID);
    let _ = fs::create_dir_all(home);
    let _ = fs::create_dir_all(&ssh);
    let _ = fs::create_dir_all(&xdg);
    let _ = fs::create_dir_all("/oath/log");
    let own = format!("{}:{}", seat::UID, seat::GID);
    let _ = Command::new("/bin/chown").args(["-R", &own, seat::HOME]).status();
    let _ = Command::new("/bin/chmod").args(["755", seat::HOME]).status();
    let _ = Command::new("/bin/chmod").args(["700", &ssh.to_string_lossy()]).status();
    let _ = Command::new("/bin/chown").args([&own, &xdg]).status();
    let _ = Command::new("/bin/chmod").args(["700", &xdg]).status();
    oath_core::seat::chown_logs();
}

fn spawn(id: &str, spec: &Svc) -> Result<Pid, String> {
    let seat = oath_core::seat::is_seat_svc(id)
        || spec.exec.iter().any(|a| {
            a.contains("run-compositor") || a.contains("/bin/sola-") || a.contains("sola-")
        });
    let mut cmd = Command::new(&spec.exec[0]);
    if spec.exec.len() > 1 {
        cmd.args(&spec.exec[1..]);
    }
    let (home, xdg, user) = if seat {
        (
            oath_core::seat::HOME,
            format!("/run/user/{}", oath_core::seat::UID),
            oath_core::seat::NAME,
        )
    } else {
        ("/root", "/run/user/0".into(), "root")
    };
    let _ = fs::create_dir_all(&xdg);
    if seat {
        let own = format!("{}:{}", oath_core::seat::UID, oath_core::seat::GID);
        let _ = Command::new("/bin/chown").args([&own, &xdg]).status();
        let _ = Command::new("/bin/chmod").args(["700", xdg.as_str()]).status();
        oath_core::seat::open_device_nodes();
    }
    let shell = if seat { "/bin/thoxa" } else { "/bin/sh" };
    cmd.env("PATH", "/bin")
        .env("HOME", home)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", shell)
        .env("PS1", "/ # ")
        .env("XDG_RUNTIME_DIR", &xdg)
        .env("SOLA_NO_SELF_WATCH", "1");
    for (k, v) in host_env() {
        cmd.env(k, v);
    }
    if seat {
        if let Some(tz) = host_timezone() {
            cmd.env("TZ", tz);
        }
    }
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let drop_priv = seat;
    unsafe {
        cmd.pre_exec(move || {
            libc::setsid();
            if drop_priv {
                let gid = oath_core::seat::GID;
                if libc::setgroups(1, &gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setgid(gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setuid(oath_core::seat::UID) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    tel(
        "init",
        "svc_spawn",
        json!({ "id": id, "seat": seat, "uid": if seat { oath_core::seat::UID } else { 0 } }),
    );
    Ok(Pid::from_raw(child.id() as i32))
}

fn reap(kids: &mut HashMap<i32, Kid>, oneshot_done: &mut HashSet<String>) {
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
        if k.spec.restart == oath_core::SvcRestart::Never {
            oneshot_done.insert(k.id.clone());
        }
        if restart {
            std::thread::sleep(Duration::from_millis(200));
            if let Ok(npid) = spawn(&k.id, &k.spec) {
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
    let mut v = serde_json::json!({ "state": state, "pid": pid, "restarts": restarts });
    let last = dir.join("last.json");
    if let Ok(extra) = oath_core::read_json::<serde_json::Value>(&last) {
        if let Some(obj) = extra.as_object() {
            for (k, val) in obj {
                if v.get(k).is_none() {
                    v[k] = val.clone();
                }
            }
        }
    }
    let _ = oath_core::write_json(&dir.join("actual.json"), &v);
}

fn fallback() {
    log("dropping to /bin/sh");
    let _ = Command::new("/bin/sh").status();
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
