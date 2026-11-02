//! Setuid helper: `home` becomes root with no password.
//!
//! `sudo [-n] [-u root|work] [--] command [args…]`. `-n` (never prompt) is
//! accepted and means nothing here; any other option is refused.
//! The command runs as root (uid, gid and groups) with a clean root
//! environment, keeping only `TERM` and `TZ` from the caller. With `-u work`
//! it runs as `work` (R55) instead, in `work`'s home: the way to that account,
//! which has no login, and it cannot come back, since `sudo` refuses it.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

use oath_core::seat::{WORK, WORK_HOME, WORK_ID};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut as_work = false;
    while let Some(f) = args.first().map(String::as_str) {
        match f {
            "-n" => {
                args.remove(0);
            }
            "-u" => {
                match args.get(1).map(String::as_str) {
                    Some("root") => as_work = false,
                    Some(WORK) => as_work = true,
                    who => {
                        eprintln!("sudo: -u takes root or work, not {}", who.unwrap_or("nothing"));
                        exit(1);
                    }
                }
                args.drain(..2);
            }
            "--" => {
                args.remove(0);
                break;
            }
            f if f.starts_with('-') => {
                eprintln!(
                    "sudo: option {f} is not supported; usage: sudo [-n] [-u root|work] [--] command [args]"
                );
                exit(1);
            }
            _ => break,
        }
    }
    if args.is_empty() {
        eprintln!("sudo: command required");
        exit(1);
    }
    let uid = unsafe { libc::getuid() };
    let euid = unsafe { libc::geteuid() };
    if uid != 0 && uid != oath_core::seat::UID {
        eprintln!("sudo: not allowed");
        exit(1);
    }
    if euid != 0 {
        eprintln!("sudo: not setuid");
        exit(1);
    }
    // Groups first (needs root), then gid, then uid; check every step.
    let ok = unsafe {
        libc::setgroups(0, std::ptr::null()) == 0 && libc::setgid(0) == 0 && libc::setuid(0) == 0
    };
    if !ok || unsafe { libc::getuid() } != 0 || unsafe { libc::getgid() } != 0 {
        eprintln!("sudo: could not become root: {}", std::io::Error::last_os_error());
        exit(1);
    }
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).env_clear().env("PATH", "/bin").env("SHELL", "/bin/sh");
    if as_work {
        // As root still: its runtime directory, as PID 1 makes one for a service.
        let xdg = format!("/run/user/{WORK_ID}");
        let _ = std::fs::create_dir_all(&xdg);
        let _ = std::os::unix::fs::chown(&xdg, Some(WORK_ID), Some(WORK_ID));
        let _ = std::fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700));
        let gid: libc::gid_t = WORK_ID;
        let ok = unsafe {
            libc::setgroups(1, &gid) == 0 && libc::setgid(WORK_ID) == 0 && libc::setuid(WORK_ID) == 0
        };
        if !ok || unsafe { libc::getuid() } != WORK_ID || unsafe { libc::geteuid() } != WORK_ID {
            eprintln!("sudo: could not become {WORK}: {}", std::io::Error::last_os_error());
            exit(1);
        }
        cmd.current_dir(WORK_HOME)
            .env("HOME", WORK_HOME)
            .env("USER", WORK)
            .env("LOGNAME", WORK)
            .env("XDG_RUNTIME_DIR", xdg);
    } else {
        cmd.env("HOME", "/root").env("USER", "root").env("LOGNAME", "root");
    }
    for keep in ["TERM", "TZ"] {
        if let Ok(v) = std::env::var(keep) {
            cmd.env(keep, v);
        }
    }
    let err = cmd.exec();
    eprintln!("sudo: {}: {err}", args[0]);
    exit(1);
}
