//! Setuid helper: `home` becomes root with no password.
//!
//! `sudo [-n] [--] command [args…]`. `-n` (never prompt) is accepted and
//! means nothing here; any other option is refused.
//! The command runs as root (uid, gid and groups) with a clean root
//! environment, keeping only `TERM` and `TZ` from the caller.

use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    while let Some(f) = args.first().map(String::as_str) {
        match f {
            "-n" => {
                args.remove(0);
            }
            "--" => {
                args.remove(0);
                break;
            }
            f if f.starts_with('-') => {
                eprintln!(
                    "sudo: option {f} is not supported; usage: sudo [-n] [--] command [args]"
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
    cmd.args(&args[1..])
        .env_clear()
        .env("PATH", "/bin")
        .env("HOME", "/root")
        .env("USER", "root")
        .env("LOGNAME", "root")
        .env("SHELL", "/bin/sh");
    for keep in ["TERM", "TZ"] {
        if let Ok(v) = std::env::var(keep) {
            cmd.env(keep, v);
        }
    }
    let err = cmd.exec();
    eprintln!("sudo: {}: {err}", args[0]);
    exit(1);
}
