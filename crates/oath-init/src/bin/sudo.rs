//! Setuid helper: `home` becomes root with no password.

use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    while args.first().map(|s| s.starts_with('-')).unwrap_or(false) {
        let f = args.remove(0);
        if f == "--" {
            break;
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
    unsafe {
        libc::setgid(0);
        libc::setuid(0);
    }
    let err = Command::new(&args[0])
        .args(&args[1..])
        .env("HOME", "/root")
        .env("USER", "root")
        .env("LOGNAME", "root")
        .exec();
    eprintln!("sudo: {err}");
    exit(1);
}
