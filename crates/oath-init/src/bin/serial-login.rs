//! Root shell on the serial console (break-glass). `svc:serial` execs this.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    let Some(f) = open_tty() else {
        // No UART. Say so once, then stay up so restart=always does not
        // back off to `failed` on a machine that has none.
        eprintln!("serial-login: no serial console (no UART at ttyS0, no hvc0); idle");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    };
    let fd = f.as_raw_fd();
    unsafe {
        libc::dup2(fd, 0);
        libc::dup2(fd, 1);
        libc::dup2(fd, 2);
        libc::setsid();
        libc::ioctl(0, libc::TIOCSCTTY, 1);
    }
    std::mem::forget(f);
    let _ =
        writeln!(std::io::stderr(), "Oath. Root on serial (break-glass). SSH is home. Try: oath");
    let _ = std::env::set_current_dir("/root");
    let err =
        Command::new("/bin/busybox").args(["sh"]).env("HOME", "/root").env("PS1", "/ # ").exec();
    eprintln!("exec sh: {err}");
    std::process::exit(1);
}

fn open_tty() -> Option<std::fs::File> {
    // The login shell is serial only.
    for name in ["ttyS0", "hvc0"] {
        if !has_uart(name) {
            continue;
        }
        if let Ok(f) = OpenOptions::new().read(true).write(true).open(format!("/dev/{name}")) {
            return Some(f);
        }
    }
    None
}

/// The 8250 driver makes ttyS0–3 whether or not a UART answers, and the
/// node opens; sysfs `type` 0 (PORT_UNKNOWN) says nothing is there. A tty
/// with no `type` (hvc0) counts as present.
fn has_uart(name: &str) -> bool {
    match std::fs::read_to_string(format!("/sys/class/tty/{name}/type")) {
        Ok(t) => t.trim() != "0",
        Err(_) => true,
    }
}
