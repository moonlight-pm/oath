//! Shell as `home` on the screen (tty1). `svc:screen` execs this as home;
//! on a desk it is the console, which has the seat once the desk ends
//! (R42). PID 1 makes `/dev/tty1` home's.
//!
//! With `host:local.autologin` off and a password set, it asks for the
//! password first and checks it with `svc:login`; without the service it
//! waits, and opens no shell. The shell is its child, not an exec: an
//! interactive shell ignores SIGTERM, so when PID 1 stops the service
//! (the desk takes the seat back) this, the session leader, ends, the
//! kernel hangs up the terminal, and the shell ends with it at once.

use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::os::fd::AsRawFd;
use std::process::Command;
use std::time::Duration;

use nix::sys::termios::{tcgetattr, tcsetattr, InputFlags, LocalFlags, OutputFlags, SetArg};
use oath_core::login::{self, Seat};
use oath_core::{Catalog, ObjectId, DEFAULT_ROOT};
use serde_json::{json, Value};

const TTY: &str = "/dev/tty1";
const KDSETMODE: libc::c_ulong = 0x4B3A;
const KD_TEXT: libc::c_ulong = 0;
const VT_ACTIVATE: libc::c_ulong = 0x5606;

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
        // After a desk: its terminal in text mode and in front.
        libc::ioctl(0, KDSETMODE, KD_TEXT);
        libc::ioctl(0, VT_ACTIVATE, 1);
    }
    std::mem::forget(f);
    sane_terminal();
    // Clear what the last session left; the boot hides the cursor.
    let _ = write!(std::io::stderr(), "\x1b[H\x1b[2J\x1b[?25h");
    if !autologin() {
        ask_password();
    }
    let desk = if Seat::current() == Seat::Console { " The desk: oath desk" } else { "" };
    let _ = writeln!(
        std::io::stderr(),
        "Oath. You are home on the screen. Root: sudo. Try: oath.{desk}"
    );
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".into());
    let _ = std::env::set_current_dir(&home);
    let shell = oath_core::seat::SHELL;
    match Command::new(shell).arg("-l").env("TERM", "linux").env("SHELL", shell).status() {
        Ok(st) => std::process::exit(st.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("{shell}: {e}");
            std::process::exit(1);
        }
    }
}

/// The line settings `stty sane` makes: a desk leaves tty1 raw, and then
/// Enter (CR) ends no line and a newline does not return the carriage.
fn sane_terminal() {
    let stdin = std::io::stdin();
    let Ok(mut t) = tcgetattr(&stdin) else { return };
    t.input_flags.remove(InputFlags::INLCR | InputFlags::IGNCR);
    t.input_flags.insert(InputFlags::ICRNL | InputFlags::IXON | InputFlags::BRKINT);
    t.output_flags.insert(OutputFlags::OPOST | OutputFlags::ONLCR);
    t.local_flags.insert(
        LocalFlags::ICANON
            | LocalFlags::ECHO
            | LocalFlags::ECHOE
            | LocalFlags::ECHOK
            | LocalFlags::ISIG
            | LocalFlags::IEXTEN,
    );
    let _ = tcsetattr(&stdin, SetArg::TCSANOW, &t);
}

/// `host:local.autologin` as applied; on when absent or unreadable.
fn autologin() -> bool {
    let id = ObjectId::new("host", "local");
    let spec = Catalog::open(DEFAULT_ROOT).ok().and_then(|cat| {
        cat.applied(&id).ok().flatten().or_else(|| cat.get(&id).ok().map(|o| o.desired))
    });
    spec.and_then(|v| v.get("autologin").and_then(Value::as_bool)).unwrap_or(true)
}

/// Until `svc:login` takes the password (or says none is set).
fn ask_password() {
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let host = host.trim();
    let mut err = std::io::stderr();
    loop {
        match login::call(&json!({ "call": "status" })) {
            Ok(v) if v.get("password") == Some(&json!(false)) => return,
            Ok(_) => {}
            Err(e) => {
                let _ = writeln!(err, "{e}; the console waits for it");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        }
        let _ = write!(err, "Password for home on {host}: ");
        let Some(pw) = read_hidden() else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        let _ = writeln!(err);
        match login::call(&json!({ "call": "check", "password": pw })) {
            Ok(v) if v.get("right") == Some(&json!(true)) => return,
            Ok(_) => {
                let _ = writeln!(err, "Wrong password.");
            }
            Err(e) => {
                let _ = writeln!(err, "{e}");
            }
        }
    }
}

/// One line from the terminal with echo off; `None` at end of input.
fn read_hidden() -> Option<String> {
    let stdin = std::io::stdin();
    let saved = tcgetattr(&stdin).ok();
    if let Some(t) = &saved {
        let mut quiet = t.clone();
        quiet.local_flags.remove(LocalFlags::ECHO);
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, &quiet);
    }
    let mut line = String::new();
    let n = stdin.lock().read_line(&mut line);
    if let Some(t) = &saved {
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, t);
    }
    match n {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
    }
}
