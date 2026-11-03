//! `writ check`: build a small project and a canary, run the attacks in a box, then look at
//! the host to see what they reached. Any attack that gets through fails it, and so does any
//! ordinary work the box wrongly stops. It reads no real secret: for the paths it names it
//! asks only whether they exist.

use crate::sandbox::{self, Plan, Session};
use crate::{landlock, serve, sys, term};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{SocketAddr, UnixListener};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(300);

struct Line {
    name: String,
    state: String,
    detail: String,
}

fn random_hex() -> String {
    let mut b = [0u8; 8];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(PartialEq, Debug)]
enum Node {
    Dir(u32),
    File(u32, Vec<u8>),
    Link(PathBuf),
}

/// The whole tree, to compare after the box ran. Never follows a symlink.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Node> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(m) = fs::symlink_metadata(&path) else { continue };
            let node = if m.file_type().is_symlink() {
                Node::Link(fs::read_link(&path).unwrap_or_default())
            } else if m.is_dir() {
                stack.push(path.clone());
                Node::Dir(m.mode())
            } else {
                Node::File(m.mode(), fs::read(&path).unwrap_or_default())
            };
            out.insert(path, node);
        }
    }
    out
}

/// The small project the attacks run against.
fn plant(project: &Path, canary: &Path, home: &Path) -> Result<(), String> {
    let write = |rel: &str, text: &str| -> Result<(), String> {
        let p = project.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    write("kept.txt", "the original\n")?;
    write("doomed.txt", "the original\n")?;
    write(".env", "API_KEY=do-not-read\n")?;
    write(".env.example", "API_KEY=\n")?;
    write("secret.pem", "do not read\n")?;
    write("id_ed25519", "do not read\n")?;
    write("nested/.env", "do not read\n")?;
    write(".ssh/id", "do not read\n")?;
    let link = |target: &Path, rel: &str| {
        std::os::unix::fs::symlink(target, project.join(rel)).map_err(|e| e.to_string())
    };
    link(canary, "link-canary")?;
    link(Path::new("../canary"), "link-rel")?;
    link(home, "link-home")?;
    if Path::new("/bin/git").exists() {
        // A tree nothing has touched yet, so this is safe on the host.
        let status = Command::new("/bin/git")
            .args(["init", "-q"])
            .current_dir(project)
            .env_clear()
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("git init failed".into());
        }
    } else {
        write(".git/config", "")?;
        write(".git/hooks/.keep", "")?;
    }
    fs::create_dir_all(project.join(".git/hooks")).map_err(|e| e.to_string())
}

/// What has to be out of the box. On a machine that lacks one, its absence proves less, and
/// the report says how many of them are really there.
fn sensitive(home: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = [
        ".ssh", ".gnupg", ".config", ".claude", ".claude.json", ".codex", ".grok", ".agents",
        ".gitconfig", ".zsh_history", ".cargo/credentials.toml", ".cargo/config.toml",
        ".local/share/writ", ".local/bin", "Projects",
    ]
    .iter()
    .map(|p| home.join(p))
    .collect();
    paths.extend(
        [
            "/oath/login", "/oath/ssh", "/oath/log", "/oath/run", "/oath/run/init.sock", "/oath/run/login.sock",
            "/etc/shadow", "/etc/ssh", "/etc/machine-id", "/etc/profile", "/root", "/work", "/run", "/run/user",
            "/boot", "/sys", "/tmp/tmux-1", "/dev/kvm", "/dev/uinput", "/dev/mem", "/dev/kmem", "/dev/port",
            "/dev/sda", "/dev/nvme0n1", "/dev/dri", "/dev/input", "/dev/snd", "/dev/fuse", "/dev/net",
            "/dev/loop0", "/dev/vfio",
        ]
        .iter()
        .map(PathBuf::from),
    );
    paths
}

/// Processes the attacks left running on the host, by their exact command line.
fn survivors(marker: &str) -> Vec<i32> {
    let want = format!("/bin/sleep\0{marker}\0");
    let Ok(procs) = fs::read_dir("/proc") else { return Vec::new() };
    procs
        .flatten()
        .filter(|p| fs::read(p.path().join("cmdline")).map(|b| b == want.as_bytes()).unwrap_or(false))
        .filter_map(|p| p.file_name().to_str()?.parse().ok())
        .collect()
}

fn is_whiteout(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_char_device() && m.rdev() == 0)
        .unwrap_or(false)
}

/// What only the host can judge, once the box has ended.
fn judge_host(
    lines: &mut Vec<Line>,
    before: &BTreeMap<PathBuf, Node>,
    project: &Path,
    upper: &Path,
    marker: &str,
) {
    let mut add = |name: &str, good: bool, detail: String| {
        lines.push(Line {
            name: name.into(),
            state: if good { "refused" } else { "LEAK" }.into(),
            detail,
        });
    };
    let after = snapshot(project);
    let moved: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|k| before.get(*k) != after.get(*k))
        .map(|k| k.display().to_string())
        .collect();
    add(
        "the real tree is byte for byte unchanged",
        moved.is_empty(),
        if moved.is_empty() { format!("{} paths compared", before.len()) } else { moved.join(", ") },
    );
    add(
        "a planted hook is not in the real .git",
        !project.join(".git/hooks/pre-commit").exists(),
        String::new(),
    );
    let held = |rel: &str, want: &str| {
        fs::read_to_string(upper.join(rel)).map(|s| s.contains(want)).unwrap_or(false)
    };
    add("the change is held in the session", held("kept.txt", "changed in the box"), String::new());
    add("the delete is a whiteout in the session", is_whiteout(&upper.join("doomed.txt")), String::new());
    add("the planted hook is held in the session", held(".git/hooks/pre-commit", "#!/bin/sh"), String::new());
    std::thread::sleep(Duration::from_millis(500));
    let left = survivors(marker);
    add("a process left behind dies with the box", left.is_empty(), format!("{left:?}"));
    for pid in left {
        unsafe { sys::kill(pid, sys::SIGKILL) };
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A directory that cannot be a project is refused before any box exists.
fn judge_refused_projects(lines: &mut Vec<Line>, home: &Path, link_home: &Path) {
    let mut bad = vec![PathBuf::from("/"), home.to_path_buf(), PathBuf::from("/etc"), home.join(".ssh"), home.join(".config")];
    bad.push(link_home.to_path_buf());
    for path in bad {
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let why = sandbox::refuse_project(&real, home);
        lines.push(Line {
            name: format!("{} as the project", path.display()),
            state: if why.is_some() { "refused" } else { "LEAK" }.into(),
            detail: why.unwrap_or_else(|| "it would be mounted".into()),
        });
    }
}

pub fn main(args: &[OsString]) -> i32 {
    let mut json = false;
    let mut verbose = false;
    for a in args {
        match a.to_str() {
            Some("--json") => json = true,
            Some("-v") | Some("--verbose") => verbose = true,
            other => {
                eprintln!("writ check: unknown argument {other:?}");
                return 2;
            }
        }
    }
    match run(json, verbose) {
        Ok(code) => code,
        Err(e) => {
            if json {
                println!("{{\"error\":{}}}", json_str(&e));
            } else {
                eprintln!("writ check: {e}");
            }
            1
        }
    }
}

fn run(json: bool, verbose: bool) -> Result<i32, String> {
    let home = sandbox::home()?;
    let tag = format!("check-{}-{}", std::process::id(), random_hex());
    let base = home.join(".cache/writ").join(&tag);
    let project = base.join("project");
    fs::create_dir_all(&project).map_err(|e| format!("{}: {e}", project.display()))?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;

    let result = attack(&home, &base, &project, &tag);

    // Whatever happened, leave nothing behind.
    let _ = fs::remove_dir_all(&base);
    let mut lines = result?;

    let leaks = lines.iter().filter(|l| l.state == "LEAK").count();
    let broken = lines.iter().filter(|l| l.state == "BROKEN").count();
    let (refused, held, works, skipped) = (
        lines.iter().filter(|l| l.state == "refused").count(),
        lines.iter().filter(|l| l.state == "held").count(),
        lines.iter().filter(|l| l.state == "works").count(),
        lines.iter().filter(|l| l.state == "skip").count(),
    );
    let abi = landlock::abi();
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string();
    let scope = fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").ok().map(|s| s.trim().to_string());
    let there = sensitive(&home).iter().filter(|p| fs::symlink_metadata(p).is_ok()).count();

    if json {
        let items: Vec<String> = lines
            .iter()
            .map(|l| {
                format!(
                    "{{\"name\":{},\"state\":{},\"detail\":{}}}",
                    json_str(&l.name),
                    json_str(&l.state.to_lowercase()),
                    json_str(&l.detail)
                )
            })
            .collect();
        println!(
            "{{\"kernel\":{},\"landlock_abi\":{abi},\"ptrace_scope\":{},\"sensitive_paths_on_host\":{there},\"leaks\":{leaks},\"broken\":{broken},\"results\":[{}]}}",
            json_str(&kernel),
            scope.as_deref().map(json_str).unwrap_or("null".into()),
            items.join(",")
        );
    } else {
        lines.sort_by_key(|l| match l.state.as_str() {
            "LEAK" => 0,
            "BROKEN" => 1,
            "skip" => 2,
            _ => 3,
        });
        for l in &lines {
            let show = verbose || !matches!(l.state.as_str(), "refused" | "held" | "works");
            if show {
                let detail = if l.detail.is_empty() { String::new() } else { format!(": {}", l.detail) };
                println!("{:8} {}{}", l.state, term::scrub(&l.name), term::scrub(&detail));
            }
        }
        println!(
            "kernel {kernel}, Landlock ABI {abi}, ptrace_scope {}; {there} of {} sensitive paths exist on this host",
            scope.as_deref().unwrap_or("none"),
            sensitive(&home).len()
        );
        println!(
            "{refused} attacks refused, {held} writes held, {works} kinds of work fine, {skipped} skipped; {leaks} got through, {broken} broken"
        );
    }
    Ok(if leaks + broken == 0 { 0 } else { 1 })
}

fn attack(home: &Path, base: &Path, project: &Path, tag: &str) -> Result<Vec<Line>, String> {
    let canary = base.join("canary");
    let canary_text = format!("writ-canary-{}", random_hex());
    fs::write(&canary, &canary_text).map_err(|e| e.to_string())?;
    plant(project, &canary, home)?;

    // A key in the environment, as another harness would keep one. The box must not see it.
    let env_text = format!("sk-or-v1-writ-check-{}", random_hex());
    std::env::set_var("OPENROUTER_API_KEY", &env_text);

    let abstract_name = format!("writ-{tag}");
    let addr = SocketAddr::from_abstract_name(abstract_name.as_bytes()).map_err(|e| e.to_string())?;
    let _listener = UnixListener::bind_addr(&addr).map_err(|e| format!("abstract socket: {e}"))?;

    let before = snapshot(project);
    let session = Session::open(tag, home)?;
    let mut plan = Plan::new(project, &session, home)?;
    plan.landlock_probe = true;

    let marker = format!("{}", 86_400_000u64 + u64::from(std::process::id()));
    let mut argv: Vec<OsString> = vec![sandbox::INNER.into(), "probe".into()];
    let mut pair = |k: &str, v: &str| {
        argv.push(k.into());
        argv.push(v.into());
    };
    pair("--canary", &canary.to_string_lossy());
    pair("--canary-text", &canary_text);
    pair("--env-text", &env_text);
    pair("--pid", &std::process::id().to_string());
    pair("--abstract", &abstract_name);
    pair("--marker", &marker);
    for p in sensitive(home) {
        pair("--absent", &p.to_string_lossy());
    }

    let mut child = plan.command(&argv).spawn().map_err(|e| format!("bwrap: {e}"))?;
    let out = Arc::new(Mutex::new(Vec::new()));
    let err = Arc::new(Mutex::new(Vec::new()));
    let (o, e) = (out.clone(), err.clone());
    let t1 = sandbox::drain(child.stdout.take().ok_or("no stdout")?, move |b| {
        o.lock().unwrap().extend_from_slice(b)
    });
    let t2 = sandbox::drain(child.stderr.take().ok_or("no stderr")?, move |b| {
        e.lock().unwrap().extend_from_slice(b)
    });
    let (status, timed_out) = sandbox::wait(&mut child, Some(LIMIT))?;
    let _ = t1.join();
    let _ = t2.join();

    let out = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    let err = String::from_utf8_lossy(&err.lock().unwrap()).into_owned();

    // The attacks must have finished, or silence would read as success.
    if timed_out || !out.lines().any(|l| l == "done") {
        let _ = session.remove();
        let tail: Vec<&str> = err.lines().rev().take(6).collect();
        return Err(format!(
            "the attacks did not finish ({status:?}{}): {}",
            if timed_out { ", timed out" } else { "" },
            term::scrub(&tail.into_iter().rev().collect::<Vec<_>>().join("; "))
        ));
    }

    let mut lines = lines_of(&out);

    judge_host(&mut lines, &before, project, &session.upper, &marker);
    judge_refused_projects(&mut lines, home, &project.join("link-home"));
    session.remove()?;
    attack_serve_box(&mut lines, home, base, &env_text)?;
    Ok(lines)
}

fn lines_of(out: &str) -> Vec<Line> {
    out.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            Some(Line {
                name: f.next()?.to_string(),
                state: f.next()?.to_string(),
                detail: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// The box `writ serve` runs a model in. It has no model here: the attacks run in its place.
fn attack_serve_box(lines: &mut Vec<Line>, home: &Path, base: &Path, env_text: &str) -> Result<(), String> {
    let (model, dir) = (base.join("model.gguf"), base.join("sock"));
    fs::write(&model, "not a model").map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = serve::probe_command(home, &model, &dir, env_text)?.output().map_err(|e| format!("bwrap: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !text.lines().any(|l| l == "done") {
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(format!("the attacks on a server's box did not finish: {}", term::scrub(&err)));
    }
    lines.extend(lines_of(&text));
    Ok(())
}
