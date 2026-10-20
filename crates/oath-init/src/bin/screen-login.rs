//! Shell as `home` on the screen (tty1). `svc:screen` execs this as home;
//! a desk profile turns it off and owns the display instead. PID 1 makes
//! `/dev/tty1` home's.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

const TTY: &str = "/dev/tty1";

fn main() {
    let f = match OpenOptions::new().read(true).write(true).open(TTY) {
        Ok(f) => f,
        Err(e) => {
            // No virtual terminal. Stay up so restart=always does not back
            // off to `failed` on a machine without one.
            eprintln!("screen-login: {TTY}: {e}; idle");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
    };
    let fd = f.as_raw_fd();
    unsafe {
        libc::dup2(fd, 0);
        libc::dup2(fd, 1);
        libc::dup2(fd, 2);
        // PID 1 already made this process a session leader.
        libc::setsid();
        libc::ioctl(0, libc::TIOCSCTTY, 0);
    }
    std::mem::forget(f);
    // Clear what the last session left; the boot hides the cursor.
    let _ = write!(
        std::io::stderr(),
        "\x1b[H\x1b[2J\x1b[?25hOath. You are home on the screen. Root: sudo. Try: oath\n"
    );
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".into());
    let _ = std::env::set_current_dir(&home);
    let shell = oath_core::seat::SHELL;
    let err = Command::new(shell).arg("-l").env("TERM", "linux").env("SHELL", shell).exec();
    eprintln!("exec {shell}: {err}");
    std::process::exit(1);
}
