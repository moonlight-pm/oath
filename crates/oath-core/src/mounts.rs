//! `host:local.mounts`: filesystems by label or UUID at paths under
//! `/mnt` and `/home`. util-linux does the work: `findfs` says whether a
//! source is plugged in, `mount` and `umount` act. PID 1 converges at boot
//! and again when a disk appears, and never unmounts until shutdown;
//! `oath apply` also remounts changed options and unmounts what left the
//! map.

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use crate::kinds::Mount;

const FINDFS: &str = "/bin/findfs";
const MOUNT: &str = "/bin/mount";
const UMOUNT: &str = "/bin/umount";

const SOURCES: &[&str] = &["LABEL=", "UUID=", "PARTLABEL=", "PARTUUID="];

/// Options that change what a mount is, not how it behaves: converge
/// decides those.
const REFUSED_OPTIONS: &[&str] = &["bind", "rbind", "move", "remount"];

/// Why `path` cannot be a mount point, if it cannot.
pub fn check_path(path: &str) -> Option<String> {
    let under = ["/mnt/", "/home/"].iter().any(|r| path.len() > r.len() && path.starts_with(r));
    if !under {
        return Some(format!("`{path}` is not under /mnt or /home"));
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
    });
    (!ok)
        .then(|| format!("source `{source}` is not LABEL=, UUID=, PARTLABEL= or PARTUUID= a value"))
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
}

/// Mount what `desired` names and is plugged in. Against `previous` (the
/// last applied map): unmount a path that left the map or changed its
/// source, and remount one whose options changed (an option taken out
/// stays until the filesystem is mounted again). PID 1 passes `previous`
/// equal to `desired`, so it only mounts.
pub fn converge(
    desired: &BTreeMap<String, Mount>,
    previous: &BTreeMap<String, Mount>,
) -> Converged {
    let mut out = Converged::default();
    // Deepest first.
    for (path, was) in previous.iter().rev() {
        let gone = desired.get(path).is_none_or(|m| m.source != was.source);
        if gone && is_mounted(path) {
            if let Err(e) = run(UMOUNT, &[path]) {
                out.errors.push(format!("umount {path}: {e}"));
            }
        }
    }
    // A parent before what is under it: `/mnt/a` sorts before `/mnt/a/b`.
    for (path, m) in desired {
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
        let Some(dev) = find(&m.source) else {
            out.waiting.push(path.clone());
            continue;
        };
        if let Err(e) = fs::create_dir_all(path) {
            out.errors.push(format!("mkdir {path}: {e}"));
            continue;
        }
        let mut args = Vec::new();
        if !m.options.is_empty() {
            args.extend(["-o", m.options.as_str()]);
        }
        args.extend([dev.as_str(), path.as_str()]);
        match run(MOUNT, &args) {
            Ok(()) => {
                out.mounted.insert(path.clone(), m.clone());
            }
            Err(e) => out.errors.push(format!("mount {} ({dev}) at {path}: {e}", m.source)),
        }
    }
    out
}

/// Is `path` a mount point now?
pub fn is_mounted(path: &str) -> bool {
    mount_points(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()).any(|p| p == path)
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
        for bad in ["/mnt", "/mnt/", "/home", "/etc/x", "/oath/x", "mnt/x", "/mntx/y", "/"] {
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
    fn options_are_plain_and_not_bind() {
        for ok in ["", "noatime", "noatime,compress=zstd:1", "subvol=/@data,ro"] {
            assert_eq!(check_options(ok), None, "{ok}");
        }
        for bad in ["noatime,", "a b", "x;y", "bind", "ro,remount", "rbind"] {
            assert!(check_options(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn mount_points_are_field_five() {
        let info = "32 2 0:29 /@ / rw,relatime - btrfs /dev/sda2 rw\n\
                    40 32 8:17 / /mnt/warehouse rw,noatime - btrfs /dev/sdb1 rw\n";
        assert_eq!(mount_points(info).collect::<Vec<_>>(), ["/", "/mnt/warehouse"]);
    }
}
