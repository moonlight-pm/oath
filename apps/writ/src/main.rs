//! writ, Oath's coding agent (R64). What exists so far is its box, where everything a model
//! will drive runs, with no network, no home, no terminal and no way to `sudo`; a local model
//! served from a box of its own (R65); and the first half of talking to it. The real tree does
//! not change until the owner says so. `docs/specs/writ.md` has the design.

mod chat;
mod check;
mod inner;
mod json;
mod landlock;
mod model;
mod probe;
mod sandbox;
mod seccomp;
mod serve;
mod sse;
mod sys;
mod term;

use chat::{Delta, Finish};
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
  writ serve --model FILE      Serve a local model from a box, on a socket only you can reach.
  writ ask [OPTIONS] PROMPT    Ask the local model one thing. It can run nothing.

Options for run:
  --project DIR    The project, mounted as an overlay. Default: this directory.
  --session NAME   Keep writes in ~/.local/share/writ/sessions/NAME. Default: drop them.
  --timeout SECS   Kill the box after this long.

Options for serve:
  --threads N      CPU threads. Default: the server's choice.
  --context N      Context size in tokens. Default: 16384.
  --gpu            Give the box the GPU (NVIDIA's), and put the whole model on it.
  --gpu-share P    Hold the server to P percent of the GPU's time, by stopping it for the rest.
  -- ARGS          Passed on to llama-server, in the box.

Options for ask:
  --tool           Offer one tool, read_file, and report the call the model makes.
  --max-tokens N   Default: 1024.
  --timeout SECS   Default: 600.
  --record FILE    Keep the stream's bytes.
  --replay FILE    Read a recorded stream instead of asking.

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

/// One question to the local model. What it says is cleaned before it reaches the terminal, and
/// nothing it asks for runs: a call is reported, with whether it could have run.
fn ask(args: &[OsString]) -> Result<i32, String> {
    let mut prompt: Option<String> = None;
    let mut tool = false;
    let mut max_tokens: u64 = 1024;
    let mut limit = Duration::from_secs(600);
    let mut record: Option<PathBuf> = None;
    let mut replay: Option<PathBuf> = None;
    let mut it = args.iter();
    let number = |v: Option<&OsString>, what: &str| -> Result<u64, String> {
        v.and_then(|s| s.to_str()).and_then(|s| s.parse().ok()).ok_or(format!("{what} takes a number"))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--tool") => tool = true,
            Some("--max-tokens") => max_tokens = number(it.next(), "--max-tokens")?,
            Some("--timeout") => limit = Duration::from_secs(number(it.next(), "--timeout")?),
            Some("--record") => record = it.next().map(PathBuf::from),
            Some("--replay") => replay = it.next().map(PathBuf::from),
            Some(s) if !s.starts_with("--") && prompt.is_none() => prompt = Some(s.to_string()),
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }

    let mut clean = term::Clean::default();
    let mut thinking = false;
    let show = |d: Delta| {
        let mut out = std::io::stdout();
        let text = match d {
            Delta::Reasoning(t) => {
                if !thinking {
                    thinking = true;
                    let _ = out.write_all(b"[thinking] ");
                }
                t
            }
            Delta::Content(t) => {
                if thinking {
                    thinking = false;
                    let _ = out.write_all(b"\n[answer] ");
                }
                t
            }
        };
        let _ = out.write_all(clean.push(text.as_bytes()).as_bytes());
        let _ = out.flush();
    };
    let turn = match &replay {
        Some(path) => model::replay(path, show)?,
        None => {
            let prompt = prompt.ok_or("writ ask needs a prompt")?;
            let socket = serve::socket(&sandbox::home()?);
            let tools = if tool { vec![model::READ_FILE] } else { vec![] };
            let body = chat::request("local", &[chat::message("user", &prompt)], &tools, max_tokens)?;
            model::send(&model::Endpoint::Local(socket), &body, limit, record.as_deref(), show)?
        }
    };
    println!();

    for c in &turn.calls {
        println!("[call] {}({})", term::scrub(&c.name), term::scrub(&c.arguments));
    }
    if !turn.calls.is_empty() {
        match turn.runnable() {
            Ok(_) => println!("[call] well formed; writ ask runs nothing"),
            Err(e) => println!("[call] would not run: {}", term::scrub(&e)),
        }
    }
    if let Some(u) = &turn.usage {
        let speed = u.tokens_per_second.map(|s| format!(", {s:.1} tokens/s")).unwrap_or_default();
        eprintln!("[usage] {} in, {} out, {} cached{speed}", u.prompt_tokens, u.completion_tokens, u.cached_tokens);
    }
    match &turn.finish {
        Finish::Stop | Finish::ToolCalls => Ok(0),
        other => {
            eprintln!("writ: the turn ended badly: {}", term::scrub(&format!("{other:?}")));
            Ok(1)
        }
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    let code = match args.first().and_then(|a| a.to_str()) {
        // The stages a box runs. They look after themselves.
        Some("inner") => inner::main(rest),
        Some("probe") => probe::main(rest),
        Some("probe-one") => probe::one(rest.first().and_then(|a| a.to_str())),
        Some("probe-serve") => serve::probe(rest),
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
        Some("serve") => {
            harden();
            match serve::main(rest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("writ: {e}");
                    inner::REFUSED_TO_START
                }
            }
        }
        Some("ask") => {
            harden();
            match ask(rest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("writ: {e}");
                    2
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
