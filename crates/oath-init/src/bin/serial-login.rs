//! Root shell on the QEMU serial. `svc:serial` execs this.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    let tty = open_tty();
    if let Some(f) = tty {
        let fd = f.as_raw_fd();
        unsafe {
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            libc::setsid();
            libc::ioctl(0, libc::TIOCSCTTY, 1);
        }
        std::mem::forget(f);
    }
    let _ =
        writeln!(std::io::stderr(), "Oath. Root on serial (break-glass). SSH is home. Try: oath");
    let _ = std::env::set_current_dir("/root");
    let err =
        Command::new("/bin/busybox").args(["sh"]).env("HOME", "/root").env("PS1", "/ # ").exec();
    eprintln!("exec sh: {err}");
    std::process::exit(1);
}

fn open_tty() -> Option<std::fs::File> {
    // Graphical VTs stay on the splash. The shell is serial only.
    for p in ["/dev/ttyS0", "/dev/hvc0"] {
        if let Ok(f) = OpenOptions::new().read(true).write(true).open(p) {
            return Some(f);
        }
    }
    None
}
