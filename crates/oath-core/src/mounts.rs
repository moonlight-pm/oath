//! `host:local.mounts`: filesystems by label or UUID, and NFS exports
//! (`nas:/mnt/alpha`), at paths under `/mnt` and `/home`, and at `/home`
//! itself. util-linux does the work: `findfs` says whether a source is
//! plugged in, `mount` and `umount` act. PID 1 converges at boot and again
//! when a disk appears, and never unmounts until shutdown; `oath apply`
//! also remounts changed options and unmounts what left the map. `/home`
//! itself is mounted only at boot (see [`HOME`]). An export is mounted by
//! the kernel's NFS client with no mount.nfs: Oath resolves the name and
//! passes the address (see [`nas`]).

use std::collections::BTreeMap;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::kinds::Mount;

const FINDFS: &str = "/bin/findfs";
const MOUNT: &str = "/bin/mount";
const UMOUNT: &str = "/bin/umount";
const GETENT: &str = "/bin/getent";
const MODPROBE: &str = "/lib/oath/modprobe";

const SOURCES: &[&str] = &["LABEL=", "UUID=", "PARTLABEL=", "PARTUUID="];

/// Home on another volume (R45). PID 1 mounts it only at boot, before
/// anything reads /home (sshd's keys, the seat's session), in place of
/// `@home` (R43); an apply or a disk appearing leaves /home as it is, and a
/// change waits for the next boot. If its source is not there at boot, PID 1
/// mounts `@home` (or leaves /home in `@`) and writes sshd's keys there.
pub const HOME: &str = crate::seat::HOME;

/// Options that change what a mount is, not how it behaves: converge
/// decides those.
const REFUSED_OPTIONS: &[&str] = &["bind", "rbind", "move", "remount"];

/// NFS's port. An export whose server does not answer here in
/// [`NAS_WAIT`] waits, as a disk that is not plugged in does.
const NFS_PORT: u16 = 2049;
const NAS_WAIT: Duration = Duration::from_secs(2);

/// The host and export of an NFS source (`nas:/mnt/alpha`), if it is one.
/// The host is a name or an IPv4 address.
pub fn nas(source: &str) -> Option<(&str, &str)> {
    let (host, export) = source.split_once(':')?;
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['-', '.'])
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    let export_ok = export.starts_with('/')
        && !export.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"');
    (host_ok && export_ok).then_some((host, export))
}

/// Why `path` cannot hold `m`, if it cannot.
pub fn check(path: &str, m: &Mount) -> Option<String> {
    let export = (path == HOME && nas(&m.source).is_some())
        .then(|| format!("/home itself is a disk, not the NFS export `{}`", m.source));
    check_path(path)
        .or_else(|| check_source(&m.source))
        .or_else(|| check_options(&m.options))
        .or(export)
}

/// Why `path` cannot be a mount point, if it cannot.
pub fn check_path(path: &str) -> Option<String> {
    let under = ["/mnt/", "/home/"].iter().any(|r| path.len() > r.len() && path.starts_with(r));
    if !under && path != HOME {
        return Some(format!("`{path}` is not under /mnt or /home, nor /home itself"));
    }
    let plain = path.len() <= 255
        && !path.ends_with('/')
        && path.split('/').skip(1).all(|c| !c.is_empty() && c != "." && c != "..")
        && !path.chars().any(|c| c.is_whitespace() || c.is_control());
    (!plain).then(|| {
        format!(
            "`{path}` is not a plain path (no empty, `.` or `..` parts, spaces or trailing `/`)"
        )
    })
}

/// Why `source` names no filesystem, if it does not.
pub fn check_source(source: &str) -> Option<String> {
    let ok = SOURCES.iter().any(|p| {
        source.strip_prefix(p).is_some_and(|v| {
            !v.is_empty() && !v.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
        })
    }) || nas(source).is_some();
    (!ok).then(|| {
        format!(
            "source `{source}` is not LABEL=, UUID=, PARTLABEL= or PARTUUID= a value, \
             nor an NFS export (host:/path)"
        )
    })
}

/// Why `options` are not mount options, if they are not.
pub fn check_options(options: &str) -> Option<String> {
    if options.is_empty() {
        return None;
    }
    for opt in options.split(',') {
        if opt.is_empty()
            || !opt.chars().all(|c| c.is_ascii_alphanumeric() || "_=:.+-/@".contains(c))
        {
            return Some(format!("options `{options}`: `{opt}` is not a mount option"));
        }
        let name = opt.split('=').next().unwrap_or(opt);
        if REFUSED_OPTIONS.contains(&name) {
            return Some(format!("options `{options}`: `{name}` is not for host:local.mounts"));
        }
    }
    None
}

/// What converging left.
#[derive(Debug, Default)]
pub struct Converged {
    /// Mounted now: the actual `mounts`.
    pub mounted: BTreeMap<String, Mount>,
    /// Paths whose source is not plugged in.
    pub waiting: Vec<String>,
    /// What failed, one line each.
    pub errors: Vec<String>,
    /// `/home`, when the map's entry is not what is mounted there now: it
    /// takes effect at the next boot (not at boot itself).
    pub next_boot: Vec<String>,
}

/// Mount what `desired` names and is plugged in. Against `previous` (the
/// last applied map): unmount a path that left the map or changed its
/// source, and remount one whose options changed (an option taken out
/// stays until the filesystem is mounted again; an export is mounted
/// again). PID 1 passes `previous` equal to `desired`, so it only mounts.
/// `/home` is mounted only when `boot` (PID 1's boot), and otherwise never
/// mounted, remounted or unmounted. An export is mounted only when `exports`:
/// its server can keep `mount` waiting, so PID 1 leaves that to `oath
/// mount-nas`, off its own loop, and here only reports one mounted.
pub fn converge(
    desired: &BTreeMap<String, Mount>,
    previous: &BTreeMap<String, Mount>,
    boot: bool,
    exports: bool,
) -> Converged {
    let mut out = Converged::default();
    // Deepest first.
    for (path, was) in previous.iter().rev() {
        if path == HOME && !boot {
            continue;
        }
        let gone = desired.get(path).is_none_or(|m| {
            m.source != was.source || (nas(&m.source).is_some() && m.options != was.options)
        });
        if gone && is_mounted(path) {
            if let Err(e) = run(UMOUNT, &[path]) {
                out.errors.push(format!("umount {path}: {e}"));
            }
        }
    }
    // A parent before what is under it: `/mnt/a` sorts before `/mnt/a/b`.
    for (path, m) in desired {
        if path == HOME && !boot {
            let now = mounted_from(path, m);
            if now {
                out.mounted.insert(path.clone(), m.clone());
            }
            if !now || previous.get(path).is_some_and(|p| p.options != m.options) {
                out.next_boot.push(path.clone());
            }
            continue;
        }
        if is_mounted(path) {
            let changed =
                previous.get(path).is_some_and(|p| p.source == m.source && p.options != m.options);
            if changed {
                let opts = if m.options.is_empty() {
                    "remount".to_string()
                } else {
                    format!("remount,{}", m.options)
                };
                if let Err(e) = run(MOUNT, &["-o", &opts, path]) {
                    out.errors.push(format!("remount {path}: {e}"));
                }
            }
            out.mounted.insert(path.clone(), m.clone());
            continue;
        }
        let found = match nas(&m.source) {
            Some(_) if !exports => None,
            Some((host, _)) => reach(host).map(|addr| {
                let _ = run(MODPROBE, &["-q", "--", "nfsv4"]);
                // -i: there is no mount.nfs; the kernel takes the options.
                let opts = nfs_options(&m.options, &addr);
                let args = ["-i", "-t", "nfs", "-o", &opts, &m.source].map(String::from).to_vec();
                (addr, args)
            }),
            None => find(&m.source).map(|dev| {
                let mut args = Vec::new();
                if !m.options.is_empty() {
                    args.extend(["-o".to_string(), m.options.clone()]);
                }
                args.push(dev.clone());
                (dev, args)
            }),
        };
        let Some((at, mut args)) = found else {
            out.waiting.push(path.clone());
            continue;
        };
        if let Err(e) = fs::create_dir_all(path) {
            out.errors.push(format!("mkdir {path}: {e}"));
            continue;
        }
        args.push(path.clone());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match run(MOUNT, &args) {
            Ok(()) => {
                out.mounted.insert(path.clone(), m.clone());
            }
            Err(e) => out.errors.push(format!("mount {} ({at}) at {path}: {e}", m.source)),
        }
    }
    out
}

/// The options an export mounts with: the entry's, the server's address
/// (the kernel resolves no name), and NFS 4.1 unless they name a version.
fn nfs_options(options: &str, addr: &str) -> String {
    let named = options.split(',').any(|o| o.starts_with("vers=") || o.starts_with("nfsvers="));
    let mut out: Vec<&str> = options.split(',').filter(|o| !o.is_empty()).collect();
    if !named {
        out.push("vers=4.1");
    }
    let addr = format!("addr={addr}");
    out.push(&addr);
    out.join(",")
}

/// The IPv4 address `host` resolves to, if its NFS server answers there.
fn reach(host: &str) -> Option<String> {
    let o = Command::new(GETENT).args(["ahostsv4", host]).output().ok()?;
    let addr = String::from_utf8_lossy(&o.stdout).split_whitespace().next()?.to_string();
    answers(&addr).is_ok().then_some(addr)
}

/// Does an NFS server answer at `addr` within [`NAS_WAIT`]?
pub fn answers(addr: &str) -> std::io::Result<()> {
    let a: SocketAddr = format!("{addr}:{NFS_PORT}")
        .parse()
        .map_err(|_| std::io::Error::other(format!("`{addr}` is not an address")))?;
    TcpStream::connect_timeout(&a, NAS_WAIT).map(drop)
}

/// `host:local.mounts` as applied in the catalog at `root`.
pub fn applied(root: &Path) -> BTreeMap<String, Mount> {
    crate::read_json::<serde_json::Value>(&root.join("objects/host/local/applied.json"))
        .ok()
        .and_then(|spec| serde_json::from_value(spec.get("mounts")?.clone()).ok())
        .unwrap_or_default()
}

/// An export mounted at `path`: its device (`0:63`, its name under
/// `/sys/fs/nfs`) and the server's address, from the topmost mount there.
pub fn nfs_mounted(path: &str) -> Option<(String, String)> {
    nfs_at(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default(), path)
}

fn nfs_at(mountinfo: &str, path: &str) -> Option<(String, String)> {
    mountinfo.lines().rev().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        let dash = f.iter().position(|x| *x == "-")?;
        if f.get(4) != Some(&path) || !f.get(dash + 1)?.starts_with("nfs") {
            return None;
        }
        let addr = f.get(dash + 3)?.split(',').find_map(|o| o.strip_prefix("addr="))?;
        Some((f.get(2)?.to_string(), addr.to_string()))
    })
}

/// Is `path` a mount point now?
pub fn is_mounted(path: &str) -> bool {
    mount_points(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()).any(|p| p == path)
}

/// Is the topmost mount at `path` the filesystem `m` names, and its
/// `subvol=` if the options name one?
pub fn mounted_from(path: &str, m: &Mount) -> bool {
    let Some(dev) = find(&m.source) else { return false };
    let subvol = m
        .options
        .split(',')
        .find_map(|o| o.strip_prefix("subvol="))
        .map(|s| format!("/{}", s.trim_start_matches('/')));
    top_mount(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default(), path)
        .is_some_and(|(root, source)| source == dev && subvol.is_none_or(|s| s == root))
}

/// The root (field 4) and source of the last mountinfo line at `path`.
fn top_mount<'a>(mountinfo: &'a str, path: &str) -> Option<(&'a str, &'a str)> {
    mountinfo.lines().rev().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        let dash = f.iter().position(|x| *x == "-")?;
        if f.get(4) != Some(&path) {
            return None;
        }
        Some((*f.get(3)?, *f.get(dash + 2)?))
    })
}

/// Field 5 of each mountinfo line. Paths here have no spaces, so the
/// kernel's `\040` escapes never matter.
fn mount_points(mountinfo: &str) -> impl Iterator<Item = &str> {
    mountinfo.lines().filter_map(|l| l.split_whitespace().nth(4))
}

/// The device a source names, if it is plugged in.
fn find(source: &str) -> Option<String> {
    let o = Command::new(FINDFS).arg(source).output().ok()?;
    let dev = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && dev.starts_with("/dev/")).then_some(dev)
}

fn run(prog: &str, args: &[&str]) -> std::result::Result<(), String> {
    let o = Command::new(prog).args(args).output().map_err(|e| format!("{prog}: {e}"))?;
    if o.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&o.stderr);
    Err(match err.trim() {
        "" => format!("{prog}: {}", o.status),
        msg => msg.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_under_mnt_and_home_only() {
        assert_eq!(check_path("/mnt/warehouse"), None);
        assert_eq!(check_path("/home/games/steam"), None);
        assert_eq!(check_path("/home"), None);
        for bad in ["/mnt", "/mnt/", "/home/", "/etc/x", "/oath/x", "mnt/x", "/mntx/y", "/"] {
            assert!(check_path(bad).unwrap().contains("not under"), "{bad}");
        }
        for bad in ["/mnt/a/", "/mnt//a", "/mnt/../etc", "/mnt/./a", "/mnt/a b"] {
            assert!(check_path(bad).unwrap().contains("not a plain path"), "{bad}");
        }
    }

    #[test]
    fn sources_name_a_filesystem() {
        for ok in ["LABEL=warehouse", "UUID=a8fa065a-947b", "PARTLABEL=x", "PARTUUID=f373"] {
            assert_eq!(check_source(ok), None, "{ok}");
        }
        for bad in ["/dev/sdb1", "LABEL=", "label=x", "LABEL=a b", "sdb1"] {
            assert!(check_source(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn exports_are_host_colon_path() {
        assert_eq!(nas("nas:/mnt/alpha"), Some(("nas", "/mnt/alpha")));
        assert_eq!(nas("10.0.0.12:/mnt/alpha/backup"), Some(("10.0.0.12", "/mnt/alpha/backup")));
        assert_eq!(check_source("nas.lan:/"), None);
        for bad in ["nas:alpha", ":/x", "-o:/x", "na s:/x", "nas:/a b", "LABEL=x:/y", "nas"] {
            assert_eq!(nas(bad), None, "{bad}");
        }
        let m = |source: &str| Mount { source: source.into(), options: String::new() };
        assert_eq!(check("/mnt/alpha", &m("nas:/mnt/alpha")), None);
        assert!(check("/home", &m("nas:/home")).unwrap().contains("is a disk"));
        assert_eq!(check("/home", &m("LABEL=home")), None);
    }

    #[test]
    fn exports_mount_with_the_address_and_a_version() {
        assert_eq!(nfs_options("", "10.0.0.12"), "vers=4.1,addr=10.0.0.12");
        let opts = nfs_options("noatime,soft", "10.0.0.12");
        assert_eq!(opts, "noatime,soft,vers=4.1,addr=10.0.0.12");
        assert_eq!(nfs_options("vers=3,nolock", "10.0.0.12"), "vers=3,nolock,addr=10.0.0.12");
    }

    #[test]
    fn an_export_is_found_in_mountinfo() {
        let info = "32 2 0:29 /@ / rw - btrfs /dev/sda2 rw\n\
                    90 32 0:63 / /mnt/alpha rw,relatime - nfs4 nas:/mnt/alpha rw,vers=4.1,\
                    hard,proto=tcp,clientaddr=10.0.0.146,addr=10.0.0.12\n";
        assert_eq!(nfs_at(info, "/mnt/alpha"), Some(("0:63".into(), "10.0.0.12".into())));
        assert_eq!(nfs_at(info, "/"), None);
    }

    #[test]
    fn options_are_plain_and_not_bind() {
        for ok in ["", "noatime", "noatime,compress=zstd:1", "subvol=/@data,ro"] {
            assert_eq!(check_options(ok), None, "{ok}");
        }
        for bad in ["noatime,", "a b", "x;y", "bind", "ro,remount", "rbind"] {
            assert!(check_options(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn top_mount_is_the_last_at_the_path() {
        let info = "32 2 0:29 /@ / rw - btrfs /dev/sda2 rw\n\
                    41 32 0:29 /@home /home rw - btrfs /dev/sda2 rw\n\
                    42 41 8:34 /@oath-home /home rw,noatime - btrfs /dev/sdc2 rw\n";
        assert_eq!(top_mount(info, "/home"), Some(("/@oath-home", "/dev/sdc2")));
        assert_eq!(top_mount(info, "/mnt/x"), None);
    }

    #[test]
    fn mount_points_are_field_five() {
        let info = "32 2 0:29 /@ / rw,relatime - btrfs /dev/sda2 rw\n\
                    40 32 8:17 / /mnt/warehouse rw,noatime - btrfs /dev/sdb1 rw\n";
        assert_eq!(mount_points(info).collect::<Vec<_>>(), ["/", "/mnt/warehouse"]);
    }
}
