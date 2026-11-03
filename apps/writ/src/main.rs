//! writ, Oath's coding agent (R64). What exists so far is its box: everything a model will
//! drive runs in one, with no network, no home, no terminal and no way to `sudo`, and the
//! real tree does not change until the owner says so. `docs/specs/writ.md` has the design.

mod check;
mod inner;
mod landlock;
mod probe;
mod sandbox;
mod seccomp;
mod sys;
mod term;

use sandbox::{Plan, Session};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "\
writ: run things in a box that cannot reach your keys, your home or the network.

Usage:
  writ check [--json] [-v]     Attack the box on this machine and report what got through.
  writ run [OPTIONS] -- CMD    Run one command in a box over the project.

Options for run:
  --project DIR    The project, mounted as an overlay. Default: this directory.
  --session NAME   Keep writes in ~/.local/share/writ/sessions/NAME. Default: drop them.
  --timeout SECS   Kill the box after this long.

Writes in a box never reach the real tree. Applying them is not built yet.
";

/// A secret in this process must stay out of `/proc/<pid>/mem`, `environ` and core files,
/// even for another program of the same user.
fn harden() {
    let none = sys::Rlimit { cur: 0, max: 0 };
    unsafe {
        sys::prctl(sys::PR_SET_DUMPABLE, 0i64, 0i64, 0i64, 0i64);
        sys::setrlimit(sys::RLIMIT_CORE, &none);
    }
    sys::cloexec_above_stderr();
}

fn run(args: &[OsString]) -> Result<i32, String> {
    let mut project: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut limit: Option<Duration> = None;
    let mut it = args.iter();
    let mut command: Vec<OsString> = Vec::new();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--project") => project = it.next().map(PathBuf::from),
            Some("--session") => name = it.next().map(|s| s.to_string_lossy().into_owned()),
            Some("--timeout") => {
                let s = it.next().and_then(|s| s.to_str()).and_then(|s| s.parse().ok());
                limit = Some(Duration::from_secs(s.ok_or("--timeout takes seconds")?));
            }
            Some("--") => {
                command = it.by_ref().cloned().collect();
                break;
            }
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    if command.is_empty() {
        return Err(USAGE.trim_end().to_string());
    }
    let home = sandbox::home()?;
    let project = match project {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let keep = name.is_some();
    let id = name.unwrap_or_else(|| format!("run-{}", std::process::id()));
    let session = Session::open(&id, &home)?;
    let plan = match Plan::new(&project, &session, &home) {
        Ok(p) => p,
        Err(e) => {
            if !keep {
                let _ = session.remove();
            }
            return Err(e);
        }
    };
    if landlock::abi() == 0 {
        eprintln!("writ: this kernel has no Landlock; the box rests on namespaces and seccomp");
    }

    let mut child = plan.command(&command).spawn().map_err(|e| format!("bwrap: {e}"))?;
    // One lock, so a line of stdout and a line of stderr cannot interleave into an escape.
    let terminal = Arc::new(Mutex::new(()));
    let relay = |to_stderr: bool, terminal: Arc<Mutex<()>>| {
        let mut clean = term::Clean::default();
        move |b: &[u8]| {
            // The end: what is left is a character the stream cut off, and it still shows.
            let text = if b.is_empty() { clean.finish() } else { clean.push(b) };
            let _hold = terminal.lock().unwrap();
            if to_stderr {
                let _ = std::io::stderr().write_all(text.as_bytes());
            } else {
                let mut out = std::io::stdout();
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            }
        }
    };
    let t1 = sandbox::drain(child.stdout.take().ok_or("no stdout")?, relay(false, terminal.clone()));
    let t2 = sandbox::drain(child.stderr.take().ok_or("no stderr")?, relay(true, terminal));
    let (status, timed_out) = sandbox::wait(&mut child, limit)?;
    let _ = t1.join();
    let _ = t2.join();

    let changed = session.changed();
    if keep {
        eprintln!("writ: session {id}: {changed} paths held in {}; your tree is unchanged", session.upper.display());
    } else {
        if changed > 0 {
            eprintln!("writ: {changed} paths written in the box and dropped; your tree is unchanged");
        }
        session.remove()?;
    }
    if timed_out {
        eprintln!("writ: killed at the time limit");
    }
    Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    let code = match args.first().and_then(|a| a.to_str()) {
        // The stages a box runs. They look after themselves.
        Some("inner") => inner::main(rest),
        Some("probe") => probe::main(rest),
        Some("probe-one") => probe::one(rest.first().and_then(|a| a.to_str())),
        Some("check") => {
            harden();
            check::main(rest)
        }
        Some("run") => {
            harden();
            match run(rest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("writ: {e}");
                    inner::REFUSED_TO_START
                }
            }
        }
        Some("-h") | Some("--help") | Some("help") | None => {
            print!("{USAGE}");
            0
        }
        Some(other) => {
            eprintln!("writ: unknown command {other}\n\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
