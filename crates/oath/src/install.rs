//! `oath install`: put the system this machine runs on a disk (R43).
//!
//! - `oath install` lists the disks: node, size, port, model, serial, and
//!   each partition's filesystem and label (as root).
//! - `oath install <disk>` says what installing there would erase and
//!   write, and changes nothing.
//! - `oath install <disk> --confirm` wipes that disk, writes a GPT with a
//!   512 MiB ESP and one btrfs filesystem (`@`, the system; `@home`,
//!   empty, mounted at `/home` by PID 1), copies this system onto it as a
//!   fresh install, writes the ESP, and makes the firmware boot it first.
//!
//! The copy is this machine's packages, store, links and catalog settings
//! from a read-only snapshot of `@`, at generation 0: no home, no
//! generations or logs, no SSH host key (a new one is made), no login
//! password, no Bluetooth pairings or DHCP leases. The firmware gets one
//! `Boot####` entry for the new ESP, first in `BootOrder`; the other
//! systems' entries (those that load a file from a disk) are removed, and
//! the firmware's own stay (efi.rs). Every other disk is only listed. It
//! refuses the disk this system runs from and a disk with anything
//! mounted.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{chown, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use oath_core::efi::{self, BootPlan, LoadOption};
use oath_core::{esp, find_bin, seat, Catalog, Error, ObjectId, Result};
use serde_json::{json, Value};

use crate::update::{cmdline_val, esp_of, extra_options, is_mounted, preflight, sh, sh_out};

pub struct Opts {
    pub disk: Option<String>,
    pub confirm: bool,
    pub json: bool,
}

/// What a fresh install leaves out of the copy, as tar patterns anchored
/// at `@`'s top: the directory stays, its contents do not.
const LEFT_OUT: &[&str] = &[
    "./home/*",
    "./root/*",
    "./tmp/*",
    "./oath/log/*",
    "./oath/run/*",
    "./oath/status/*",
    "./oath/ssh/*",
    "./oath/login/*",
    "./oath/objects/snap/*",
    "./var/lib/*/*",
];

const ESP_SIZE: &str = "512MiB";

#[derive(Debug, Clone)]
struct Disk {
    path: String,
    name: String,
    size: u64,
    model: String,
    serial: String,
    port: String,
    removable: bool,
    /// This system runs from it.
    running: bool,
    /// What is mounted from it (mount points).
    mounted: Vec<String>,
    parts: Vec<Part>,
}

#[derive(Debug, Clone)]
struct Part {
    path: String,
    size: u64,
    fstype: Option<String>,
    label: Option<String>,
}

impl Disk {
    fn json(&self) -> Value {
        json!({
            "path": self.path,
            "size": self.size,
            "model": self.model,
            "serial": self.serial,
            "port": self.port,
            "removable": self.removable,
            "running": self.running,
            "mounted": self.mounted,
            "parts": self.parts.iter().map(|p| json!({
                "path": p.path, "size": p.size, "fstype": p.fstype, "label": p.label,
            })).collect::<Vec<_>>(),
        })
    }
}

pub fn run(root: &Path, cat: &Catalog, opts: Opts) -> Result<i32> {
    let is_root = unsafe { libc::geteuid() } == 0;
    let disks = disks(is_root)?;
    let Some(arg) = &opts.disk else {
        list(&disks, is_root, opts.json);
        return Ok(0);
    };
    let disk = resolve(&disks, arg)?;
    if disk.running {
        return Err(Error::hint(
            format!("{} holds the system this machine runs", disk.path),
            "install onto another disk (oath install lists them)",
        ));
    }
    if !disk.mounted.is_empty() {
        return Err(Error::hint(
            format!("{} has mounted filesystems: {}", disk.path, disk.mounted.join(" ")),
            "unmount them first",
        ));
    }
    if !is_root {
        return Err(Error::hint(
            "installing reads every partition and the firmware's boot entries",
            format!("sudo oath install {}", disk.path),
        ));
    }
    if root != Path::new(oath_core::DEFAULT_ROOT) {
        return Err(Error::Msg(format!(
            "oath install copies the running system; its catalog is {}",
            oath_core::DEFAULT_ROOT
        )));
    }
    preflight(root, cat)?;
    let cmdline = fs::read_to_string("/proc/cmdline")?;
    let subvol = cmdline_val(&cmdline, "oath.subvol").unwrap_or_else(|| "@".into());
    if subvol != oath_core::LIVE_SUBVOL {
        return Err(Error::hint(
            format!("this machine booted {subvol}, an older boot, not the current one"),
            "boot the current entry (Oath) and install from it",
        ));
    }
    let top = Path::new(oath_core::BTRFS_TOP);
    if !top.join("@").is_dir() {
        return Err(Error::Msg(format!("{} has no @ (btrfs top level not mounted)", top.display())));
    }
    let root_dev = cmdline_val(&cmdline, "oath.root").ok_or_else(|| {
        Error::hint("the running kernel has no oath.root=", "boot this machine from an Oath ESP")
    })?;
    let node = if root_dev.starts_with("UUID=") {
        sh_out(Command::new(find_bin("findfs")).arg(&root_dev))?
    } else {
        root_dev.clone()
    };
    let src_esp = esp_of(&node)
        .ok_or_else(|| Error::Msg(format!("no ESP beside this system's root {node}")))?;
    let system = system_summary(cat)?;
    let firmware = firmware_plan(false)?;

    if !opts.confirm {
        preview(disk, &system, firmware.as_ref(), opts.json);
        return Ok(0);
    }
    let done = install(root, cat, disk, &src_esp)?;
    let fw = firmware_write(disk, &done.esp_guid)?;
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "disk": disk.path,
                "model": disk.model,
                "serial": disk.serial,
                "root": format!("UUID={}", done.uuid),
                "host_key": done.host_key,
                "firmware": fw,
            }))?
        );
    } else {
        println!();
        println!("Oath is on {}: {}.", disk.path, ident(disk));
        println!("Its root is UUID={}; its SSH host key is", done.uuid);
        println!("  {}", done.host_key);
        match fw {
            Some(_) => println!(
                "Oath is first in the firmware's boot list. `sudo poweroff`, then remove \
                 this system's disk if it is removable (it runs from it until then) and \
                 start the machine; if the firmware starts something else, move Oath \
                 first in its setup."
            ),
            None => println!(
                "The firmware's boot list is unchanged: choose the disk in its boot menu."
            ),
        }
    }
    Ok(0)
}

// ---- disks ----

fn disks(is_root: bool) -> Result<Vec<Disk>> {
    let out = sh_out(Command::new(find_bin("lsblk")).args([
        "-J",
        "-b",
        "-o",
        "NAME,PATH,SIZE,TYPE,TRAN,RM,MODEL,SERIAL,MOUNTPOINTS",
    ]))?;
    let v: Value = serde_json::from_str(&out)?;
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let root_node = cmdline_val(&cmdline, "oath.root").and_then(|r| {
        if r.starts_with("UUID=") {
            sh_out(Command::new(find_bin("findfs")).arg(&r)).ok()
        } else {
            Some(r)
        }
    });
    let swaps = fs::read_to_string("/proc/swaps").unwrap_or_default();
    let mut disks = Vec::new();
    for d in v["blockdevices"].as_array().into_iter().flatten() {
        let name = s(&d["name"]);
        if d["type"] != "disk" || name.starts_with("zram") || name.starts_with("ram") {
            continue;
        }
        let path = s(&d["path"]);
        let serial = match s(&d["serial"]) {
            x if x.is_empty() => sysfs_serial(&name),
            x => x,
        };
        let mut mounted = mountpoints(d);
        let mut parts = Vec::new();
        let mut running = false;
        for p in d["children"].as_array().into_iter().flatten() {
            let ppath = s(&p["path"]);
            let mps = mountpoints(p);
            running |= root_node.as_deref() == Some(ppath.as_str()) || mps.iter().any(|m| m == "/");
            mounted.extend(mps);
            if swaps.lines().any(|l| l.split_whitespace().next() == Some(ppath.as_str())) {
                mounted.push(format!("{ppath} (swap)"));
            }
            let (fstype, label) = if is_root { probe(&ppath) } else { (None, None) };
            parts.push(Part { path: ppath, size: p["size"].as_u64().unwrap_or(0), fstype, label });
        }
        disks.push(Disk {
            name,
            size: d["size"].as_u64().unwrap_or(0),
            model: s(&d["model"]),
            serial,
            port: s(&d["tran"]),
            removable: d["rm"] == json!(true),
            running,
            mounted,
            parts,
            path,
        });
    }
    Ok(disks)
}

/// Without udev, lsblk has no serial for a virtio disk; the kernel keeps
/// it in sysfs.
fn sysfs_serial(name: &str) -> String {
    ["serial", "device/serial"]
        .iter()
        .filter_map(|f| fs::read_to_string(format!("/sys/block/{name}/{f}")).ok())
        .map(|t| t.trim().to_string())
        .find(|t| !t.is_empty())
        .unwrap_or_default()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

fn mountpoints(d: &Value) -> Vec<String> {
    d["mountpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(String::from))
        .collect()
}

/// A partition's filesystem type and label, read from its superblock.
fn probe(part: &str) -> (Option<String>, Option<String>) {
    let out = Command::new(find_bin("blkid"))
        .args(["-o", "export", part])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let get = |k: &str| {
        out.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(String::from))
    };
    (get("TYPE"), get("LABEL"))
}

fn resolve<'a>(disks: &'a [Disk], arg: &str) -> Result<&'a Disk> {
    let found: Vec<&Disk> = disks
        .iter()
        .filter(|d| {
            d.path == arg
                || d.name == arg
                || (!d.serial.is_empty() && d.serial == arg)
        })
        .collect();
    match found.as_slice() {
        [d] => Ok(d),
        _ => Err(Error::hint(
            format!("no disk is {arg:?}"),
            "oath install lists the disks; name one by its node (/dev/sdb) or serial",
        )),
    }
}

/// Sizes as the label on a drive says them (decimal).
fn size(b: u64) -> String {
    let gb = b as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else if gb >= 10.0 {
        format!("{gb:.0} GB")
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

fn ident(d: &Disk) -> String {
    let mut parts = vec![];
    if !d.model.is_empty() {
        parts.push(d.model.clone());
    }
    parts.push(size(d.size));
    if !d.serial.is_empty() {
        parts.push(format!("serial {}", d.serial));
    }
    if !d.port.is_empty() {
        parts.push(d.port.clone());
    }
    parts.join(", ")
}

fn part_line(p: &Part) -> String {
    let short = p.path.trim_start_matches("/dev/");
    let mut line = format!("  {short:<10} {:>9}", size(p.size));
    match (&p.fstype, &p.label) {
        (Some(t), Some(l)) => line.push_str(&format!("  {t} \"{l}\"")),
        (Some(t), None) => line.push_str(&format!("  {t}")),
        _ => {}
    }
    line
}

fn list(disks: &[Disk], is_root: bool, as_json: bool) {
    if as_json {
        let all: Vec<Value> = disks.iter().map(Disk::json).collect();
        println!("{}", serde_json::to_string_pretty(&json!({ "disks": all })).unwrap_or_default());
        return;
    }
    println!("{:<14} {:>9}  {:<5} {:<24} serial", "disk", "size", "port", "model");
    for d in disks {
        let mut line = format!(
            "{:<14} {:>9}  {:<5} {:<24} {}",
            d.path,
            size(d.size),
            if d.port.is_empty() { "-" } else { &d.port },
            if d.model.is_empty() { "-" } else { &d.model },
            if d.serial.is_empty() { "-" } else { &d.serial },
        );
        if d.running {
            line.push_str("   <- this system runs from it");
        } else if !d.mounted.is_empty() {
            line.push_str("   <- mounted");
        }
        println!("{line}");
        for p in &d.parts {
            println!("{}", part_line(p));
        }
    }
    println!();
    if !is_root {
        println!("(sudo oath install shows each filesystem's type and label)");
    }
    println!(
        "`sudo oath install <disk>` says what installing there would erase and write; \
         `--confirm` does it. <disk> is a node (/dev/sdb) or a serial."
    );
}

// ---- what is copied ----

struct System {
    /// The release it runs, when it pulled one (`/oath/output.json`); an
    /// install from the build host has none until its first update.
    output: Option<String>,
    profile: String,
    hostname: String,
    packages: usize,
}

fn system_summary(cat: &Catalog) -> Result<System> {
    let out: Value = oath_core::read_json(&cat.root().join("output.json")).unwrap_or(Value::Null);
    let host = cat.get(&ObjectId::new("host", "local"))?.desired;
    let mut packages = 0;
    for id in cat.ls(Some("pkg"))? {
        if cat.get(&id)?.desired["present"] == json!(true) {
            packages += 1;
        }
    }
    Ok(System {
        output: out["output"].as_str().map(String::from),
        profile: host["profile"].as_str().unwrap_or("").to_string(),
        hostname: host["hostname"].as_str().unwrap_or("oath").to_string(),
        packages,
    })
}

fn preview(d: &Disk, sys: &System, fw: Option<&(BootPlan, Vec<(u16, LoadOption)>)>, as_json: bool) {
    let (p1, p2) = (part_node(&d.path, 1), part_node(&d.path, 2));
    if as_json {
        let entries = |v: &[(u16, String)]| -> Vec<Value> {
            v.iter()
                .map(|(n, s)| json!({ "entry": efi::boot_name(*n), "description": s }))
                .collect()
        };
        let fwj = fw.map(|(plan, _)| {
            json!({
                "add": efi::boot_name(plan.add),
                "remove": entries(&plan.remove),
                "keep": entries(&plan.keep),
            })
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "disk": d.json(),
                "writes": {"esp": p1, "root": p2, "subvolumes": ["@", "@home"]},
                "copies": {
                    "output": sys.output,
                    "profile": sys.profile,
                    "hostname": sys.hostname,
                    "packages": sys.packages,
                },
                "firmware": fwj,
                "confirm": format!("sudo oath install {} --confirm", d.path),
            }))
            .unwrap_or_default()
        );
        return;
    }
    println!("install on {}: {}", d.path, ident(d));
    if d.parts.is_empty() {
        println!("erases: the whole disk (it has no partitions)");
    } else {
        println!("erases the whole disk:");
        for p in &d.parts {
            println!("{}", part_line(p));
        }
    }
    println!("writes:");
    println!(
        "  {:<10} {:>9}  ESP (FAT32 \"OATHESP\"): oath-efi, the kernel, the initrd",
        short(&p1),
        size(512 << 20)
    );
    println!(
        "  {:<10} {:>9}  btrfs \"oath\": @ (the system), @home (empty, at /home)",
        short(&p2),
        "the rest"
    );
    let release = match &sys.output {
        Some(o) => format!("output {o} ({})", sys.profile),
        None => format!("the {} profile", sys.profile),
    };
    println!(
        "copies this system: {release}, {} packages, host {}",
        sys.packages, sys.hostname
    );
    println!(
        "  and not its home, generations and logs, SSH host key (it gets a new one), \
         login password, Bluetooth pairings or DHCP leases"
    );
    match fw {
        Some((plan, _)) => {
            println!("firmware boot entries:");
            println!(
                "  adds     {} Oath ({} {}), first",
                efi::boot_name(plan.add),
                short(&p1),
                efi::OATH_LOADER
            );
            for (n, desc) in &plan.remove {
                println!("  removes  {} {desc}", efi::boot_name(*n));
            }
            for (n, desc) in &plan.keep {
                println!("  keeps    {} {desc}", efi::boot_name(*n));
            }
        }
        None => println!(
            "firmware boot entries: unchanged (this machine did not boot through UEFI)"
        ),
    }
    println!("every other disk is left as it is.");
    println!("`sudo oath install {} --confirm` does it.", d.path);
}

fn short(p: &str) -> &str {
    p.trim_start_matches("/dev/")
}

/// Partition `n` of `disk` (`/dev/sdb` → `/dev/sdb1`, `/dev/nvme0n1` →
/// `/dev/nvme0n1p1`).
fn part_node(disk: &str, n: u32) -> String {
    if disk.ends_with(|c: char| c.is_ascii_digit()) {
        format!("{disk}p{n}")
    } else {
        format!("{disk}{n}")
    }
}

// ---- the install ----

struct Installed {
    uuid: String,
    /// The ESP's GPT partition GUID, which the firmware's entry names.
    esp_guid: String,
    host_key: String,
}

fn install(root: &Path, cat: &Catalog, d: &Disk, src_esp: &str) -> Result<Installed> {
    let (p1, p2) = (part_node(&d.path, 1), part_node(&d.path, 2));
    let uuid = new_uuid()?;
    let esp_uuid = new_uuid()?;
    eprintln!(">> partition {}: GPT, ESP {ESP_SIZE} ({p1}), btrfs ({p2})", d.path);
    let _ = Command::new(find_bin("wipefs")).args(["-a", "-q"]).arg(&d.path).status();
    let script = format!(
        "label: gpt\nsize={ESP_SIZE}, type=uefi, name=ESP, uuid={esp_uuid}\ntype=linux, name=oath\n"
    );
    let mut sf = Command::new(find_bin("sfdisk"))
        .args(["--wipe", "always", "--wipe-partitions", "always", "--quiet"])
        .arg(&d.path)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Msg(format!("sfdisk: {e}")))?;
    sf.stdin.take().expect("piped").write_all(script.as_bytes())?;
    let st = sf.wait()?;
    if !st.success() {
        return Err(Error::Msg(format!("sfdisk {}: {st}", d.path)));
    }
    let _ = Command::new(find_bin("blockdev")).args(["--rereadpt", &d.path]).status();
    for _ in 0..20 {
        if Path::new(&p1).exists() && Path::new(&p2).exists() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if !Path::new(&p1).exists() || !Path::new(&p2).exists() {
        return Err(Error::Msg(format!("{p1} and {p2} did not appear after partitioning")));
    }
    sh(Command::new(find_bin("mkfs.fat"))
        .args(["-F", "32", "-n", "OATHESP", &p1])
        .stdout(Stdio::null()))?;
    sh(Command::new(find_bin("mkfs.btrfs"))
        .args(["-q", "-f", "-L", "oath", "-U", &uuid, &p2])
        .stdout(Stdio::null()))?;

    let work = root.join("run/install");
    let (dst_top, dst_esp, src_esp_mnt) =
        (work.join("fs"), work.join("esp"), work.join("from-esp"));
    for m in [&dst_top, &dst_esp, &src_esp_mnt] {
        fs::create_dir_all(m)?;
    }
    let snap = Path::new(oath_core::BTRFS_TOP).join("@install");
    let result = (|| -> Result<Installed> {
        sh(Command::new(find_bin("mount"))
            .args(["-t", "btrfs", "-o", "subvolid=5", &p2])
            .arg(&dst_top))?;
        let btrfs = find_bin("btrfs");
        for sub in ["@", "@home"] {
            sh(Command::new(&btrfs)
                .args(["subvolume", "create"])
                .arg(dst_top.join(sub))
                .stdout(Stdio::null()))?;
        }
        let at = dst_top.join("@");

        eprintln!(">> snapshot this system's @ (read-only)");
        if snap.exists() {
            let _ = Command::new(&btrfs)
                .args(["subvolume", "delete"])
                .arg(&snap)
                .stdout(Stdio::null())
                .status();
        }
        {
            let _lock = cat.lock()?;
            sh(Command::new(&btrfs)
                .args(["subvolume", "snapshot", "-r", &format!("{}/@", oath_core::BTRFS_TOP)])
                .arg(&snap)
                .stdout(Stdio::null()))?;
        }

        eprintln!(">> copy the system onto {p2} (its home, history and keys stay here)");
        copy_tree(&snap, &at)?;
        fresh_state(&at, &dst_top.join("@home"))?;
        let host_key = host_key(&at)?;
        if let Ok(c) = Catalog::open(at.join("oath")) {
            let _ = c.write_index();
        }

        eprintln!(">> ESP: oath-efi, the kernel and initrd this system booted");
        sh(Command::new(find_bin("mount")).args(["-t", "vfat", &p1]).arg(&dst_esp))?;
        sh(Command::new(find_bin("mount"))
            .args(["-t", "vfat", "-o", "ro", src_esp])
            .arg(&src_esp_mnt))?;
        write_esp(&src_esp_mnt, &dst_esp, &uuid)?;
        Ok(Installed { uuid: uuid.clone(), esp_guid: esp_uuid.clone(), host_key })
    })();
    let _ = Command::new("sync").status();
    for m in [&src_esp_mnt, &dst_esp, &dst_top] {
        if is_mounted(m) {
            let _ = Command::new(find_bin("umount")).arg(m).status();
        }
    }
    if snap.exists() {
        let _ = Command::new(find_bin("btrfs"))
            .args(["subvolume", "delete"])
            .arg(&snap)
            .stdout(Stdio::null())
            .status();
    }
    result
}

fn new_uuid() -> Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/uuid")?.trim().to_string())
}

/// GNU tar from the snapshot into the new `@`: owners by number, modes,
/// the setuid bit, links and extended attributes, without LEFT_OUT.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let tar = find_bin("tar");
    let mut c = Command::new(&tar);
    c.arg("-C").arg(from).args([
        "--numeric-owner",
        "--xattrs",
        "--xattrs-include=*",
        "--anchored",
    ]);
    for p in LEFT_OUT {
        c.arg(format!("--exclude={p}"));
    }
    c.args(["-cf", "-", "."]).stdout(Stdio::piped());
    let mut src = c.spawn().map_err(|e| Error::Msg(format!("tar: {e}")))?;
    let out = src.stdout.take().expect("piped");
    let st = Command::new(&tar)
        .arg("-C")
        .arg(to)
        .args(["--numeric-owner", "--xattrs", "--xattrs-include=*", "-xpf", "-"])
        .stdin(Stdio::from(out))
        .status()
        .map_err(|e| Error::Msg(format!("tar: {e}")))?;
    let sst = src.wait()?;
    if !st.success() || !sst.success() {
        return Err(Error::Msg(format!("copying the system failed (tar: {sst}, {st})")));
    }
    Ok(())
}

/// What a fresh install has where the copy left things out: generation 0,
/// an empty apply log, an empty home (its own subvolume) with `.ssh` for
/// PID 1 to fill from `ssh:local`, and root's `.ssh`.
fn fresh_state(at: &Path, home: &Path) -> Result<()> {
    let snap = at.join("oath/objects/snap/current");
    fs::create_dir_all(&snap)?;
    let gen0 = serde_json::to_string_pretty(&json!({ "generation": 0 }))?;
    fs::write(snap.join("desired.json"), &gen0)?;
    fs::write(snap.join("applied.json"), &gen0)?;
    fs::write(
        snap.join("meta.json"),
        serde_json::to_string_pretty(&json!({
            "id": "snap:current", "kind": "snap", "name": "current",
            "safety": "mutate", "status": "in-sync",
        }))?,
    )?;
    fs::create_dir_all(at.join("oath/log"))?;
    fs::write(at.join("oath/log/apply.jsonl"), "")?;
    fs::create_dir_all(at.join("root/.ssh"))?;
    fs::set_permissions(at.join("root"), fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(at.join("root/.ssh"), fs::Permissions::from_mode(0o700))?;
    fs::create_dir_all(at.join("home"))?;
    for (dir, mode) in [(home.to_path_buf(), 0o755), (home.join(".ssh"), 0o700)] {
        fs::create_dir_all(&dir)?;
        chown(&dir, Some(seat::UID), Some(seat::GID))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

/// The new system's SSH host key, made here so the installer can say it.
fn host_key(at: &Path) -> Result<String> {
    let key = at.join(oath_core::SSH_HOST_KEY.trim_start_matches('/'));
    if let Some(dir) = key.parent() {
        fs::create_dir_all(dir)?;
    }
    sh(Command::new(find_bin("ssh-keygen"))
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "", "-f"])
        .arg(&key))?;
    let mut pubk = key.into_os_string();
    pubk.push(".pub");
    let line = fs::read_to_string(PathBuf::from(pubk))?;
    let mut w = line.split_whitespace();
    Ok(format!("{} {}", w.next().unwrap_or(""), w.next().unwrap_or("")))
}

/// The boot this system runs: oath-efi, kernel and initrd from its ESP,
/// and one entry that names the new root by its UUID.
fn write_esp(from: &Path, to: &Path, uuid: &str) -> Result<()> {
    fs::create_dir_all(to.join("EFI/BOOT"))?;
    fs::create_dir_all(to.join("loader/entries"))?;
    for f in ["EFI/BOOT/BOOTX64.EFI", "vmlinuz", "initrd.gz"] {
        fs::copy(from.join(f), to.join(f))
            .map_err(|e| Error::Msg(format!("copy {f} from this system's ESP: {e}")))?;
    }
    let current = fs::read_to_string(from.join("loader/entries/oath.conf")).unwrap_or_default();
    let extra = extra_options(&esp::parse_bls(&current).options);
    let loader = fs::read_to_string(from.join("loader/loader.conf"))
        .unwrap_or_else(|_| esp::LOADER_CONF_METAL.to_string());
    fs::write(to.join("loader/loader.conf"), loader)?;
    let entry = esp::current_bls(&format!("UUID={uuid}"), &extra);
    fs::write(to.join("loader/entries/oath.conf"), entry)?;
    fs::write(to.join("loader/oath-boots"), esp::format_oath_boots(&["oath.conf".to_string()]))?;
    Ok(())
}

// ---- the firmware's boot entries ----

/// efivarfs at its place for the length of `f`: read-only to look, read-
/// write only to write. Oath leaves it unmounted otherwise.
fn with_efivars<T>(write: bool, f: impl FnOnce(&Path) -> Result<T>) -> Result<Option<T>> {
    if !Path::new("/sys/firmware/efi").is_dir() {
        return Ok(None);
    }
    let dir = Path::new(efi::EFIVARS);
    let ours = !is_mounted(dir);
    if ours {
        let opts = if write { "nosuid,nodev,noexec" } else { "ro,nosuid,nodev,noexec" };
        sh(Command::new(find_bin("mount"))
            .args(["-t", "efivarfs", "-o", opts, "efivarfs"])
            .arg(dir))?;
    }
    let r = f(dir);
    if ours {
        let _ = Command::new(find_bin("umount")).arg(dir).status();
    }
    r.map(Some)
}

fn firmware_plan(write: bool) -> Result<Option<(BootPlan, Vec<(u16, LoadOption)>)>> {
    with_efivars(write, |dir| {
        let entries = efi::read_entries(dir)?;
        let order = efi::read_boot_order(dir)?;
        Ok((efi::plan_boot(&entries, &order), entries))
    })
}

/// Add Oath's entry, put it first, then remove the other systems': the
/// machine boots Oath even if a removal fails.
fn firmware_write(d: &Disk, esp_guid: &str) -> Result<Option<Value>> {
    let p1 = part_node(&d.path, 1);
    let pname = short(&p1).to_string();
    let read = |f: &str| -> Result<u64> {
        let p = format!("/sys/class/block/{pname}/{f}");
        let text = fs::read_to_string(&p)?;
        text.trim().parse().map_err(|_| Error::Msg(format!("{p} is not a number")))
    };
    let (start, len) = (read("start")?, read("size")?);
    let lbs_file = format!("/sys/class/block/{}/queue/logical_block_size", d.name);
    let lbs: u64 = fs::read_to_string(lbs_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(512);
    // sysfs counts 512-byte sectors; the device path counts the disk's blocks.
    let (start, len) = (start * 512 / lbs, len * 512 / lbs);
    let path = efi::disk_file_path(1, start, len, esp_guid, efi::OATH_LOADER)
        .ok_or_else(|| Error::Msg(format!("{p1}'s partition GUID {esp_guid:?} is not a GUID")))?;
    let entry = LoadOption {
        attributes: efi::LOAD_OPTION_ACTIVE,
        description: "Oath".into(),
        path,
        optional: vec![],
    };
    with_efivars(true, |dir| {
        let entries = efi::read_entries(dir)?;
        let order = efi::read_boot_order(dir)?;
        let plan = efi::plan_boot(&entries, &order);
        let name = efi::boot_name(plan.add);
        eprintln!(">> firmware: add {name} Oath ({} {}), first", short(&p1), efi::OATH_LOADER);
        efi::write_var(dir, &name, efi::BOOT_ATTRS, &entry.encode())?;
        efi::write_var(dir, "BootOrder", efi::BOOT_ATTRS, &efi::encode_boot_order(&plan.order))?;
        let next = efi::read_var(dir, "BootNext")?
            .map(|(_, d)| efi::parse_boot_order(&d))
            .and_then(|v| v.first().copied());
        let mut removed = Vec::new();
        for (n, desc) in &plan.remove {
            eprintln!(">> firmware: remove {} {desc}", efi::boot_name(*n));
            match efi::delete_var(dir, &efi::boot_name(*n)) {
                Ok(()) => removed.push(json!({"entry": efi::boot_name(*n), "description": desc})),
                Err(e) => eprintln!("warning: {e} (Oath boots first anyway)"),
            }
            if next == Some(*n) {
                let _ = efi::delete_var(dir, "BootNext");
            }
        }
        Ok(json!({
            "added": name,
            "order": plan.order.iter().map(|n| efi::boot_name(*n)).collect::<Vec<_>>(),
            "removed": removed,
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_are_named_as_the_kernel_names_them() {
        assert_eq!(part_node("/dev/sdb", 1), "/dev/sdb1");
        assert_eq!(part_node("/dev/vdb", 2), "/dev/vdb2");
        assert_eq!(part_node("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(part_node("/dev/mmcblk0", 2), "/dev/mmcblk0p2");
    }

    #[test]
    fn sizes_read_as_the_drive_label_does() {
        assert_eq!(size(500_107_862_016), "500 GB");
        assert_eq!(size(4_000_787_030_016), "4.0 TB");
        assert_eq!(size(128_320_801_792), "128 GB");
        assert_eq!(size(2_147_483_648), "2.1 GB");
        assert_eq!(size(536_870_912), "537 MB");
    }

    fn disk(path: &str, serial: &str) -> Disk {
        Disk {
            path: path.into(),
            name: path.trim_start_matches("/dev/").into(),
            size: 1,
            model: String::new(),
            serial: serial.into(),
            port: String::new(),
            removable: false,
            running: false,
            mounted: vec![],
            parts: vec![],
        }
    }

    #[test]
    fn a_disk_is_named_by_node_or_serial() {
        let ds = vec![
            disk("/dev/sda", "AA250217S304TB00619"),
            disk("/dev/sdb", "21385X801116"),
            disk("/dev/sdd", ""),
        ];
        assert_eq!(resolve(&ds, "/dev/sdb").unwrap().path, "/dev/sdb");
        assert_eq!(resolve(&ds, "sdb").unwrap().path, "/dev/sdb");
        assert_eq!(resolve(&ds, "21385X801116").unwrap().path, "/dev/sdb");
        assert!(resolve(&ds, "/dev/sdc").is_err());
        assert!(resolve(&ds, "").is_err());
    }
}
