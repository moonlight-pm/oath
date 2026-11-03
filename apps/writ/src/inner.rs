//! The box's second stage. bwrap builds the namespaces and mounts and executes this, and this
//! closes what remains before anything the model chose runs. Any failure stops here: a box
//! that is not what it should be runs nothing.

use crate::{landlock, seccomp, sys};
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// The exit code of a box that refused to start, so it is not read as the command's.
pub const REFUSED_TO_START: i32 = 125;

/// Enough for a large parallel build, and it stops a fork bomb. Since Linux 5.14 this counts
/// processes in the box's own user namespace, not all of the user's.
const MAX_PROCESSES: u64 = 4096;

fn stop(why: &str) -> ! {
    eprintln!("writ: the box refused to start: {why}");
    std::process::exit(REFUSED_TO_START);
}

fn capabilities_are_empty() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    ["CapInh:", "CapPrm:", "CapEff:", "CapAmb:"].iter().all(|field| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(field))
            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
            == Some(0)
    })
}

pub fn main(args: &[OsString]) -> ! {
    let mut writable: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    let mut command: Vec<OsString> = Vec::new();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--write") => match it.next() {
                Some(p) => writable.push(PathBuf::from(p)),
                None => stop("--write takes a path"),
            },
            Some("--") => {
                command = it.by_ref().cloned().collect();
                break;
            }
            _ => stop("bad arguments"),
        }
    }
    if command.is_empty() {
        stop("no command");
    }

    // A terminal is a way out: ioctls on it, and escape sequences a person reads.
    for fd in 0..3 {
        if unsafe { sys::isatty(fd) } == 1 {
            stop("a terminal reached the box");
        }
    }
    if unsafe { sys::prctl(sys::PR_SET_NO_NEW_PRIVS, 1i64, 0i64, 0i64, 0i64) } != 0 {
        stop("no_new_privs");
    }
    unsafe { sys::prctl(sys::PR_CAP_AMBIENT, sys::PR_CAP_AMBIENT_CLEAR_ALL, 0i64, 0i64, 0i64) };
    if !capabilities_are_empty() {
        stop("a capability survived bwrap");
    }
    sys::cloexec_above_stderr();
    let core = sys::Rlimit { cur: 0, max: 0 };
    let processes = sys::Rlimit { cur: MAX_PROCESSES, max: MAX_PROCESSES };
    if unsafe { sys::setrlimit(sys::RLIMIT_CORE, &core) } != 0
        || unsafe { sys::setrlimit(sys::RLIMIT_NPROC, &processes) } != 0
    {
        stop("rlimit");
    }

    let paths: Vec<&std::path::Path> = writable.iter().map(|p| p.as_path()).collect();
    if let Err(e) = landlock::restrict(&paths) {
        stop(&e);
    }
    // Last: it refuses things the steps above may call.
    if let Err(e) = seccomp::load_filter() {
        stop(&e);
    }

    let e = Command::new(&command[0]).args(&command[1..]).exec();
    eprintln!("writ: {}: {e}", command[0].to_string_lossy());
    std::process::exit(127);
}
