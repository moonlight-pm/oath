//! The gate: what a session wrote, as plain data, and the owner's way to take it. The upper
//! directory is the box's output, so it is walked with no symlink followed and no program run on
//! it. Nothing reaches the real tree except through `apply`, and `undo` puts it back.

use crate::agent::{now, Meta};
use crate::json::{self, n, s, Value};
use crate::sandbox::{self, Session};
use crate::term;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsString};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const O_NOFOLLOW: i32 = 0o400000;
const O_NONBLOCK: i32 = 0o4000;
/// A text diff of anything larger only says that it changed.
const MAX_DIFF_BYTES: u64 = 1 << 20;
/// More lines than this in the part that differs and a diff shows it as one replacement.
const MAX_DIFF_LINES: usize = 4000;
const CONTEXT_LINES: usize = 3;

extern "C" {
    fn lgetxattr(path: *const std::ffi::c_char, name: *const std::ffi::c_char, value: *mut u8, size: usize) -> isize;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    File,
    Link(PathBuf),
    MakeDir,
    Delete,
    RemoveDir,
    /// A FIFO, a socket or a device: never applied.
    Unusual(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub kind: Kind,
    /// Why the owner should look before this one is applied. Empty for most.
    pub flags: Vec<&'static str>,
}

/// A name that runs later: a build, a hook, an instruction to the next agent. No list can decide
/// these are safe, and agents must edit them, so they wait until the owner names them. Tools find
/// their configuration in any parent directory, so these count at any depth.
fn runs_later(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let in_tool_dir = path.components().any(|c| {
        matches!(c.as_os_str().to_str(), Some(".cargo" | ".github" | ".gitlab" | ".claude" | ".vscode" | ".idea" | ".husky" | ".devcontainer"))
    });
    in_tool_dir
        || matches!(
            name.as_str(),
            "build.rs" | "Cargo.toml" | "Cargo.lock" | "configure" | "package.json" | "package-lock.json" | "go.mod" | "go.sum" | ".envrc"
                | "AGENTS.md" | "CLAUDE.md" | ".mcp.json" | ".gitignore" | ".gitmodules" | ".gitattributes" | ".editorconfig" | "pkg.toml"
                | "flake.nix" | "Dockerfile" | "Justfile" | "justfile" | "CMakeLists.txt" | "meson.build" | "Rakefile" | "Gemfile" | "setup.py"
                | "conftest.py" | "pyproject.toml" | ".npmrc" | ".pre-commit-config.yaml"
        )
        || name.starts_with("Makefile")
        || name.ends_with(".sh")
        || name.ends_with(".plan")
        || name.ends_with(".pth")
}

fn flags_for(path: &Path, kind: &Kind, mode_changed: bool) -> Vec<&'static str> {
    let mut f = Vec::new();
    if runs_later(path) {
        f.push("runs later");
    }
    if matches!(kind, Kind::Link(_)) {
        f.push("a symlink");
    }
    if mode_changed {
        f.push("its mode changed");
    }
    if matches!(kind, Kind::Unusual(_)) {
        f.push("not a file");
    }
    f
}

fn is_opaque(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut v = [0u8; 2];
    for name in [c"user.overlay.opaque", c"trusted.overlay.opaque"] {
        let k = unsafe { lgetxattr(c.as_ptr(), name.as_ptr(), v.as_mut_ptr(), v.len()) };
        if k >= 1 && v[0] == b'y' {
            return true;
        }
    }
    false
}

/// A file from the upper directory, if it is still a plain one: no link followed, no FIFO waited on.
fn read_plain(path: &Path) -> Result<Vec<u8>, String> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a plain file", path.display()));
    }
    let mut out = Vec::new();
    f.read_to_end(&mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out)
}

/// Everything under `real` that deleting `rel` would take: files first, each directory after what
/// it holds. Nothing followed.
fn deletions(real: &Path, rel: &Path, out: &mut Vec<Change>) {
    let at = real.join(rel);
    let Ok(m) = fs::symlink_metadata(&at) else { return };
    if m.is_dir() {
        let mut names: Vec<OsString> = fs::read_dir(&at).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        names.sort();
        for name in names {
            deletions(real, &rel.join(name), out);
        }
        out.push(Change { path: rel.to_path_buf(), kind: Kind::RemoveDir, flags: flags_for(rel, &Kind::RemoveDir, false) });
    } else {
        out.push(Change { path: rel.to_path_buf(), kind: Kind::Delete, flags: flags_for(rel, &Kind::Delete, false) });
    }
}

fn walk(upper: &Path, real: &Path, rel: &Path, out: &mut Vec<Change>) -> Result<(), String> {
    let dir = upper.join(rel);
    let mut entries: Vec<(OsString, fs::Metadata)> = Vec::new();
    for e in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let m = e.metadata().map_err(|err| format!("{}: {err}", e.path().display()))?;
        entries.push((e.file_name(), m));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, m) in &entries {
        let path = rel.join(name);
        // The model can commit in the box. That history does not leave it, at any depth: a
        // repository's config names programs git runs, in `sub/` as much as at the top.
        if name.as_os_str() == ".git" {
            continue;
        }
        let at_real = real.join(&path);
        let at_upper = upper.join(&path);
        let t = m.file_type();
        let real_m = fs::symlink_metadata(&at_real).ok();
        if t.is_char_device() && m.rdev() == 0 {
            deletions(real, &path, out);
        } else if t.is_dir() {
            match &real_m {
                Some(r) if r.is_dir() => {}
                Some(_) => {
                    deletions(real, &path, out);
                    out.push(Change { path: path.clone(), kind: Kind::MakeDir, flags: flags_for(&path, &Kind::MakeDir, false) });
                }
                None => out.push(Change { path: path.clone(), kind: Kind::MakeDir, flags: flags_for(&path, &Kind::MakeDir, false) }),
            }
            // An opaque directory replaced the real one whole: what it does not hold is gone.
            if is_opaque(&at_upper) && real_m.as_ref().is_some_and(|r| r.is_dir()) {
                let held: HashSet<OsString> = fs::read_dir(&at_upper).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
                let mut gone: Vec<OsString> = fs::read_dir(&at_real).map(|d| d.flatten().map(|e| e.file_name()).filter(|n| !held.contains(n)).collect()).unwrap_or_default();
                gone.sort();
                for n in gone {
                    deletions(real, &path.join(n), out);
                }
            }
            walk(upper, real, &path, out)?;
        } else if t.is_symlink() {
            let to = fs::read_link(&at_upper).map_err(|e| format!("{}: {e}", at_upper.display()))?;
            if real_m.as_ref().is_some_and(|r| r.file_type().is_symlink()) && fs::read_link(&at_real).ok().as_ref() == Some(&to) {
                continue;
            }
            if real_m.as_ref().is_some_and(|r| r.is_dir()) {
                deletions(real, &path, out);
            }
            let kind = Kind::Link(to);
            out.push(Change { path: path.clone(), flags: flags_for(&path, &kind, false), kind });
        } else if t.is_file() {
            let exec = |mode: u32| mode & 0o111 != 0;
            let special = m.mode() & 0o7000 != 0;
            let same_mode = real_m.as_ref().is_none_or(|r| exec(r.mode()) == exec(m.mode()) && r.is_file()) && !special;
            if let Some(r) = &real_m {
                if r.is_file() && r.len() == m.len() && same_mode && read_plain(&at_upper)? == fs::read(&at_real).map_err(|e| format!("{}: {e}", at_real.display()))? {
                    continue;
                }
                if r.is_dir() {
                    deletions(real, &path, out);
                }
            }
            let mode_changed = !same_mode || (real_m.is_none() && exec(m.mode()));
            let mut flags = flags_for(&path, &Kind::File, mode_changed);
            // What cannot be read cannot be reviewed, and may be code: a `.pyc`, an `.so`, a binary.
            if !read_plain(&at_upper).is_ok_and(|b| is_text(&b)) {
                flags.push("binary");
            }
            out.push(Change { path: path.clone(), kind: Kind::File, flags });
        } else {
            let what = if t.is_fifo() { "a FIFO" } else if t.is_socket() { "a socket" } else { "a device" };
            let kind = Kind::Unusual(what);
            out.push(Change { path: path.clone(), flags: flags_for(&path, &kind, false), kind });
        }
    }
    Ok(())
}

/// Names the real tree's git ignores, asked of the real tree: names are data on its stdin, and it
/// never sees the upper directory. With no repository or no git, only what is plainly build output.
fn ignored(real: &Path, paths: &[(PathBuf, bool)]) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let asked = if real.join(".git").exists() {
        Command::new("/bin/git")
            .env_clear()
            .env("PATH", "/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(real)
            .args(["check-ignore", "--stdin", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    } else {
        None
    };
    if let Some(mut c) = asked {
        let mut stdin = c.stdin.take().expect("piped");
        // A slash says it is a directory, which a pattern like `target/` needs if the real tree has none.
        let mut names = Vec::new();
        for (p, is_dir) in paths {
            names.extend_from_slice(p.as_os_str().as_bytes());
            if *is_dir {
                names.push(b'/');
            }
            names.push(0);
        }
        let feed = std::thread::spawn(move || {
            let _ = stdin.write_all(&names);
        });
        let mut said = Vec::new();
        let _ = c.stdout.take().expect("piped").read_to_end(&mut said);
        let _ = feed.join();
        let _ = c.wait();
        for name in said.split(|b| *b == 0).filter(|n| !n.is_empty()) {
            out.insert(PathBuf::from(std::ffi::OsStr::from_bytes(name)));
        }
    } else {
        for (p, _) in paths {
            if p.components().any(|c| matches!(c.as_os_str().to_str(), Some("target" | "node_modules"))) {
                out.insert(p.clone());
            }
        }
    }
    out
}

/// What the session would change in the real tree, in the order `apply` would do it.
pub fn changes(upper: &Path, real: &Path) -> Result<Vec<Change>, String> {
    let mut all = Vec::new();
    walk(upper, real, Path::new(""), &mut all)?;
    let names: Vec<(PathBuf, bool)> = all.iter().map(|c| (c.path.clone(), matches!(c.kind, Kind::MakeDir | Kind::RemoveDir))).collect();
    let skip = ignored(real, &names);
    all.retain(|c| !skip.contains(&c.path));
    Ok(all)
}

fn letter(c: &Change, real: &Path) -> &'static str {
    match &c.kind {
        Kind::File if fs::symlink_metadata(real.join(&c.path)).is_ok() => "M",
        Kind::File | Kind::MakeDir => "A",
        Kind::Link(_) => "L",
        Kind::Delete | Kind::RemoveDir => "D",
        Kind::Unusual(_) => "?",
    }
}

fn name(p: &Path) -> String {
    term::scrub(&p.to_string_lossy())
}

/// One line about a change, for a person, cleaned.
pub fn line(c: &Change, real: &Path) -> String {
    let mut t = format!("{} {}", letter(c, real), name(&c.path));
    if let Kind::Link(to) = &c.kind {
        t.push_str(&format!(" -> {}", name(to)));
    }
    if let Kind::Unusual(what) = &c.kind {
        t.push_str(&format!(" ({what}: never applied)"));
    }
    if !c.flags.is_empty() {
        t.push_str(&format!("  [{}]", c.flags.join(", ")));
    }
    t
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Op {
    Same,
    Del,
    Ins,
}

/// The shortest edit between two lists of lines, from their longest common subsequence, past what
/// they share at each end. A part too long to compare comes back as one replacement.
fn line_ops(a: &[&str], b: &[&str]) -> Vec<(Op, usize, usize)> {
    let start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let end = a[start..].iter().rev().zip(b[start..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (ma, mb) = (&a[start..a.len() - end], &b[start..b.len() - end]);
    let mut ops: Vec<(Op, usize, usize)> = (0..start).map(|i| (Op::Same, i, i)).collect();
    if ma.len() > MAX_DIFF_LINES || mb.len() > MAX_DIFF_LINES {
        ops.extend((0..ma.len()).map(|i| (Op::Del, start + i, 0)));
        ops.extend((0..mb.len()).map(|j| (Op::Ins, 0, start + j)));
    } else {
        let w = mb.len() + 1;
        let mut t = vec![0u16; (ma.len() + 1) * w];
        for i in (0..ma.len()).rev() {
            for j in (0..mb.len()).rev() {
                t[i * w + j] = if ma[i] == mb[j] { t[(i + 1) * w + j + 1] + 1 } else { t[(i + 1) * w + j].max(t[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < ma.len() || j < mb.len() {
            if i < ma.len() && j < mb.len() && ma[i] == mb[j] {
                ops.push((Op::Same, start + i, start + j));
                i += 1;
                j += 1;
            } else if j == mb.len() || (i < ma.len() && t[(i + 1) * w + j] >= t[i * w + j + 1]) {
                ops.push((Op::Del, start + i, 0));
                i += 1;
            } else {
                ops.push((Op::Ins, 0, start + j));
                j += 1;
            }
        }
    }
    ops.extend((0..end).map(|k| (Op::Same, a.len() - end + k, b.len() - end + k)));
    ops
}

/// A unified diff of two texts, with what it shows cleaned for a terminal.
pub fn unified(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let ops = line_ops(&a, &b);
    let mut out = String::new();
    let mut i = 0;
    while i < ops.len() {
        if ops[i].0 == Op::Same {
            i += 1;
            continue;
        }
        let from = i.saturating_sub(CONTEXT_LINES);
        // A hunk runs until the lines that do not change are more than two contexts long.
        let mut to = i;
        let mut last = i;
        while to < ops.len() && to - last <= CONTEXT_LINES * 2 {
            if ops[to].0 != Op::Same {
                last = to;
            }
            to += 1;
        }
        let end = (last + CONTEXT_LINES + 1).min(ops.len());
        let (mut la, mut lb) = (0, 0);
        let mut body = String::new();
        for (op, x, y) in &ops[from..end] {
            let (sign, text) = match op {
                Op::Same => {
                    la += 1;
                    lb += 1;
                    (' ', a[*x])
                }
                Op::Del => {
                    la += 1;
                    ('-', a[*x])
                }
                Op::Ins => {
                    lb += 1;
                    ('+', b[*y])
                }
            };
            body.push(sign);
            body.push_str(&term::scrub(text));
            body.push('\n');
        }
        let first_a = ops[..from].iter().filter(|o| o.0 != Op::Ins).count();
        let first_b = ops[..from].iter().filter(|o| o.0 != Op::Del).count();
        out.push_str(&format!("@@ -{},{la} +{},{lb} @@\n{body}", first_a + 1, first_b + 1));
        i = end;
    }
    out
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn is_text(b: &[u8]) -> bool {
    !b.iter().take(8192).any(|x| *x == 0) && std::str::from_utf8(b).is_ok()
}

/// Print what a session would change. Names and contents come from the box, so all of it is cleaned.
pub fn show(upper: &Path, real: &Path, list: &[Change], names_only: bool, out: &mut dyn Write) {
    if list.is_empty() {
        let _ = writeln!(out, "nothing to apply");
        return;
    }
    for c in list {
        let _ = writeln!(out, "{}", line(c, real));
    }
    if names_only {
        return;
    }
    for c in list {
        if c.kind != Kind::File {
            continue;
        }
        let _ = writeln!(out, "\n--- {}", name(&c.path));
        let m = fs::symlink_metadata(upper.join(&c.path));
        if m.is_ok_and(|m| m.len() > MAX_DIFF_BYTES) {
            let _ = writeln!(out, "[too large to show]");
            continue;
        }
        let after = match read_plain(&upper.join(&c.path)) {
            Ok(b) => b,
            Err(e) => {
                let _ = writeln!(out, "[{}]", term::scrub(&e));
                continue;
            }
        };
        let before = match fs::symlink_metadata(real.join(&c.path)) {
            Ok(r) if r.is_file() && r.len() > MAX_DIFF_BYTES => {
                let _ = writeln!(out, "[too large to show]");
                continue;
            }
            Ok(r) if r.is_file() => fs::read(real.join(&c.path)).unwrap_or_default(),
            _ => Vec::new(),
        };
        if !is_text(&after) || !is_text(&before) {
            let _ = writeln!(out, "[binary: {} bytes to {} bytes]", before.len(), after.len());
            continue;
        }
        let _ = write!(out, "{}", unified(&String::from_utf8_lossy(&before), &String::from_utf8_lossy(&after)));
    }
}

/// A session that exists, without making one by asking for it.
fn existing(id: &str, home: &Path) -> Result<Session, String> {
    if !home.join(".local/share/writ/sessions").join(id).join("upper").is_dir() {
        return Err(format!("there is no session {id:?}: see `writ sessions`"));
    }
    Session::open(id, home)
}

pub fn diff_main(args: &[OsString]) -> Result<i32, String> {
    let mut id = None;
    let mut names_only = false;
    for a in args {
        match a.to_str() {
            Some("--names") => names_only = true,
            Some(t) if !t.starts_with("--") && id.is_none() => id = Some(t.to_string()),
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    let id = id.ok_or("writ diff needs a session")?;
    let session = existing(&id, &sandbox::home()?)?;
    let _hold = session.lock()?;
    let meta = Meta::load(&session.dir)?;
    let list = changes(&session.upper, &meta.project)?;
    show(&session.upper, &meta.project, &list, names_only, &mut std::io::stdout());
    Ok(0)
}

/// Every session, newest last, and what each holds.
pub fn sessions_main() -> Result<i32, String> {
    sessions(&sandbox::home()?, &mut std::io::stdout())
}

pub fn sessions(home: &Path, out: &mut dyn Write) -> Result<i32, String> {
    let dir = home.join(".local/share/writ/sessions");
    let mut found: Vec<(u64, String)> = Vec::new();
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let Ok(meta) = Meta::load(&e.path()) else { continue };
        let upper = e.path().join("upper");
        let held = changes(&upper, &meta.project).map(|c| c.len().to_string()).unwrap_or_else(|_| "?".into());
        found.push((meta.started, format!("{}  {held} changes  {}", term::scrub(&id), name(&meta.project))));
    }
    found.sort();
    for (_, t) in &found {
        let _ = writeln!(out, "{t}");
    }
    if found.is_empty() {
        let _ = writeln!(out, "no sessions");
    }
    Ok(0)
}

/// Every parent of `rel` under the project must be a real directory or not there yet: a session
/// can have replaced a symlink with a directory, and the real symlink could lead out of the project.
fn parents_are_plain(real: &Path, rel: &Path) -> Result<(), String> {
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("{} is not a plain relative path", name(rel)));
    }
    let mut at = real.to_path_buf();
    if let Some(parent) = rel.parent() {
        for part in parent.components() {
            at.push(part);
            match fs::symlink_metadata(&at) {
                Err(_) => return Ok(()),
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(format!("{} goes through {}, which is not a directory", name(rel), name(&at))),
            }
        }
    }
    Ok(())
}

/// What `undo` needs for one path. One line of JSON each, oldest first.
fn undo_line(path: &Path, backup: Option<&str>, mode: u32, link: Option<&Path>, after: Option<&str>, made_dir: bool) -> Result<String, String> {
    let text = |p: &Path| p.to_str().map(str::to_string).ok_or(format!("{} is not UTF-8", name(p)));
    let mut pairs = vec![("path".to_string(), s(&text(path)?)), ("mode".to_string(), n(mode as u64))];
    if let Some(b) = backup {
        pairs.push(("backup".to_string(), s(b)));
    }
    if let Some(l) = link {
        pairs.push(("link".to_string(), s(&text(l)?)));
    }
    if let Some(h) = after {
        pairs.push(("after".to_string(), s(h)));
    }
    if made_dir {
        pairs.push(("made_dir".to_string(), Value::Bool(true)));
    }
    Ok(Value::Obj(pairs).text())
}

/// One of the session's logs: undo.jsonl, what `apply` did, or restored.jsonl, what `undo` put back.
fn read_log(dir: &Path, file: &str) -> Result<Vec<Value>, String> {
    let p = dir.join(file);
    match fs::read_to_string(&p) {
        Err(_) => Ok(Vec::new()),
        Ok(t) => t.lines().map(|l| json::parse(l).map_err(|e| format!("{}: {e}", p.display()))).collect(),
    }
}

/// A file's contents, as the logs record them.
fn file_hash(bytes: &[u8]) -> String {
    format!("{:016x}", hash_of(bytes))
}

/// Where a link points, as the logs record it.
fn link_hash(to: &Path) -> String {
    format!("link {:016x}", hash_of(to.as_os_str().as_bytes()))
}

/// What is at a path in the real tree, as the logs record it: a file's contents, or where a link
/// points, hashed. Nothing for a directory, or where nothing is.
fn found_at(at: &Path) -> Option<String> {
    let m = fs::symlink_metadata(at).ok()?;
    if m.file_type().is_symlink() {
        fs::read_link(at).ok().map(|to| link_hash(&to))
    } else if m.is_file() {
        fs::read(at).ok().map(|b| file_hash(&b))
    } else {
        None
    }
}

/// What this session last left at each path, by `apply`, whose entries are given, or by `undo`.
/// `undo` takes each entry out of undo.jsonl as it is done, so what it put back or left to the owner
/// is older than anything still in there. An entry with no hash left nothing of its own, and forgets
/// what was left there before.
fn left_by_session(dir: &Path, applied: &[Value]) -> Result<HashMap<String, String>, String> {
    let mut left = HashMap::new();
    for v in read_log(dir, "restored.jsonl")?.iter().chain(applied) {
        let Some(p) = v.get("path").and_then(|p| p.str()) else { continue };
        match v.get("after").and_then(|h| h.str()) {
            Some(h) => left.insert(p.to_string(), h.to_string()),
            None => left.remove(p),
        };
    }
    Ok(left)
}

/// The real tree's file changed since the session began, and not by this session's own `apply` or
/// `undo`: the model worked from something else, so applying would lose the owner's change.
fn base_moved(real: &Path, rel: &Path, started: u64, left: &HashMap<String, String>) -> bool {
    let at = real.join(rel);
    let Ok(m) = fs::symlink_metadata(&at) else { return false };
    if m.is_dir() || (m.mtime().max(0) as u64) < started {
        return false;
    }
    match rel.to_str().and_then(|p| left.get(p)) {
        Some(h) => found_at(&at).as_ref() != Some(h),
        None => true,
    }
}

/// Put one change into the real tree, whole or not at all, and say how to take it back.
fn apply_one(upper: &Path, real: &Path, undo_dir: &Path, seq: usize, c: &Change) -> Result<Vec<String>, String> {
    parents_are_plain(real, &c.path)?;
    let at = real.join(&c.path);
    let parent = at.parent().ok_or("a path with no parent")?;
    let mut undo = Vec::new();
    // Parents a new file needs, so undo can take them away again.
    if !matches!(c.kind, Kind::Delete | Kind::RemoveDir) {
        let mut missing = Vec::new();
        let mut p = c.path.parent();
        while let Some(q) = p.filter(|q| !q.as_os_str().is_empty()) {
            if fs::symlink_metadata(real.join(q)).is_ok() {
                break;
            }
            missing.push(q.to_path_buf());
            p = q.parent();
        }
        for q in missing.iter().rev() {
            fs::create_dir(real.join(q)).map_err(|e| format!("{}: {e}", name(q)))?;
            undo.push(undo_line(q, None, 0, None, None, true)?);
        }
    }
    // What is there now, kept so undo can bring it back.
    let old = fs::symlink_metadata(&at).ok();
    let backup = format!("{seq}");
    let (mut had_backup, mut mode, mut old_link) = (None, 0, None);
    if let Some(m) = &old {
        mode = m.mode() & 0o777;
        if m.is_file() {
            fs::copy(&at, undo_dir.join(&backup)).map_err(|e| format!("keeping {}: {e}", name(&c.path)))?;
            had_backup = Some(backup.as_str());
        } else if m.file_type().is_symlink() {
            old_link = Some(fs::read_link(&at).map_err(|e| e.to_string())?);
        }
    }
    let temp = |tag: &str| parent.join(format!(".writ-{tag}-{}-{seq}", std::process::id()));
    // What this leaves at the path, so a later `apply` or `undo` can tell it from the owner's change.
    let mut after = None;
    match &c.kind {
        Kind::File => {
            let bytes = read_plain(&upper.join(&c.path))?;
            let m = fs::symlink_metadata(upper.join(&c.path)).map_err(|e| e.to_string())?;
            after = Some(file_hash(&bytes));
            let tmp = temp("new");
            let done = (|| -> std::io::Result<()> {
                let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
                f.write_all(&bytes)?;
                f.sync_all()?;
                // Setuid, setgid and sticky never come out of a box.
                f.set_permissions(fs::Permissions::from_mode(m.mode() & 0o777))?;
                fs::rename(&tmp, &at)
            })();
            if let Err(e) = done {
                let _ = fs::remove_file(&tmp);
                return Err(format!("{}: {e}", name(&c.path)));
            }
        }
        Kind::Link(to) => {
            after = Some(link_hash(to));
            let tmp = temp("link");
            std::os::unix::fs::symlink(to, &tmp).map_err(|e| format!("{}: {e}", name(&c.path)))?;
            fs::rename(&tmp, &at).map_err(|e| format!("{}: {e}", name(&c.path)))?;
        }
        Kind::MakeDir => {
            if old.is_none() {
                fs::create_dir(&at).map_err(|e| format!("{}: {e}", name(&c.path)))?;
            }
        }
        Kind::Delete => {
            if old.is_some() {
                fs::remove_file(&at).map_err(|e| format!("{}: {e}", name(&c.path)))?;
            }
        }
        Kind::RemoveDir => {
            // Ignored files may still be in it, and those stay, with the directory.
            if fs::remove_dir(&at).is_err() {
                return Ok(undo);
            }
        }
        Kind::Unusual(_) => return Err(format!("{} is not a file", name(&c.path))),
    }
    if c.kind == Kind::RemoveDir {
        let path = c.path.to_str().ok_or("not UTF-8")?;
        undo.push(Value::Obj(vec![("path".to_string(), s(path)), ("mode".to_string(), n(mode as u64)), ("was_dir".to_string(), Value::Bool(true))]).text());
    } else {
        undo.push(undo_line(&c.path, had_backup, mode, old_link.as_deref(), after.as_deref(), c.kind == Kind::MakeDir && old.is_none())?);
    }
    Ok(undo)
}

pub fn apply_main(args: &[OsString]) -> Result<i32, String> {
    let mut id = None;
    let mut all = false;
    let mut named: Vec<PathBuf> = Vec::new();
    for a in args {
        match a.to_str() {
            Some("--all") => all = true,
            Some(t) if t.starts_with("--") => return Err(format!("unknown argument {t}")),
            _ if id.is_none() => id = Some(a.to_string_lossy().into_owned()),
            _ => named.push(PathBuf::from(a)),
        }
    }
    let id = id.ok_or("writ apply needs a session")?;
    let session = existing(&id, &sandbox::home()?)?;
    let _hold = session.lock()?;
    let meta = Meta::load(&session.dir)?;
    apply(&session.upper, &session.dir, &meta, all, &named, &mut std::io::stdout())
}

/// Apply what was asked. Names given are the owner's word for those, flagged or not, and `--all` for
/// everything; with neither, what is flagged waits. Nothing happens if anything asked for cannot.
pub fn apply(upper: &Path, dir: &Path, meta: &Meta, all: bool, named: &[PathBuf], out: &mut dyn Write) -> Result<i32, String> {
    let real = &meta.project;
    let list = changes(upper, real)?;
    let undo_so_far = read_log(dir, "undo.jsonl")?;
    let left = left_by_session(dir, &undo_so_far)?;
    for p in named {
        if !list.iter().any(|c| c.path == *p || c.path.starts_with(p)) {
            return Err(format!("the session does not change {}", name(p)));
        }
    }
    let chosen = |c: &Change| {
        if named.is_empty() { all || c.flags.is_empty() } else { named.iter().any(|p| c.path == *p || c.path.starts_with(p)) }
    };
    let (take, waiting): (Vec<&Change>, Vec<&Change>) = list.iter().partition(|c| chosen(c));
    let mut problems = Vec::new();
    for c in &take {
        if let Kind::Unusual(what) = c.kind {
            problems.push(format!("{} is {what}, which is never applied", name(&c.path)));
        }
        if base_moved(real, &c.path, meta.started, &left) {
            problems.push(format!("{} changed in your tree since this session began", name(&c.path)));
        }
        if let Err(e) = parents_are_plain(real, &c.path) {
            problems.push(e);
        }
        if c.path.to_str().is_none() {
            problems.push(format!("{} is not UTF-8", name(&c.path)));
        }
    }
    if !problems.is_empty() {
        return Err(format!("nothing was applied:\n  {}", problems.join("\n  ")));
    }
    let undo_dir = dir.join("undo");
    fs::create_dir_all(&undo_dir).map_err(|e| format!("{}: {e}", undo_dir.display()))?;
    let mut log = fs::OpenOptions::new().create(true).append(true).open(dir.join("undo.jsonl")).map_err(|e| e.to_string())?;
    for (i, c) in take.iter().enumerate() {
        let seq = undo_so_far.len() + i + (now() as usize % 1000) * 100_000;
        let said = line(c, real);
        let lines = apply_one(upper, real, &undo_dir, seq, c).map_err(|e| format!("stopped at {}: {e}. What came before it is applied; `writ undo` takes it back", name(&c.path)))?;
        for l in lines {
            writeln!(log, "{l}").map_err(|e| e.to_string())?;
        }
        let _ = writeln!(out, "{said}");
    }
    if !waiting.is_empty() {
        let _ = writeln!(out, "\nThese wait for your word, by name or with --all:");
        for c in &waiting {
            let _ = writeln!(out, "  {}", line(c, real));
        }
    }
    let _ = writeln!(out, "{} applied, {} waiting. You commit; `writ undo` takes it back.", take.len(), waiting.len());
    Ok(0)
}

pub fn undo_main(args: &[OsString]) -> Result<i32, String> {
    let id = args.first().and_then(|a| a.to_str()).ok_or("writ undo needs a session")?;
    let session = existing(id, &sandbox::home()?)?;
    let _hold = session.lock()?;
    let meta = Meta::load(&session.dir)?;
    undo(&session.dir, &meta.project, &mut std::io::stdout())
}

/// Take back one entry of undo.jsonl, whole or not at all: nothing if its path is put back, or why
/// the path stays the owner's.
fn undo_one(dir: &Path, real: &Path, seq: usize, v: &Value) -> Result<Option<String>, String> {
    let path = v.get("path").and_then(|p| p.str()).ok_or("undo.jsonl: an entry with no path")?;
    let rel = PathBuf::from(path);
    if let Err(e) = parents_are_plain(real, &rel) {
        return Ok(Some(format!("{e}, and stays")));
    }
    let at = real.join(&rel);
    let parent = at.parent().ok_or("a path with no parent")?;
    let is_true = |k: &str| v.get(k) == Some(&Value::Bool(true));
    // An entry with no hash left no file or link, and only one that made a directory left a
    // directory: anything else there now is the owner's.
    let there = fs::symlink_metadata(&at).ok();
    let as_applied = match v.get("after").and_then(|h| h.str()) {
        Some(h) => found_at(&at).as_deref() == Some(h),
        None if is_true("made_dir") => there.as_ref().is_none_or(|m| m.is_dir()),
        None => there.is_none(),
    };
    if !as_applied {
        return Ok(Some(format!("{} changed after it was applied, and stays", name(&rel))));
    }
    let (backup, link) = (v.get("backup").and_then(|b| b.str()), v.get("link").and_then(|l| l.str()));
    if is_true("made_dir") {
        let _ = fs::remove_dir(&at);
    } else if is_true("was_dir") {
        let mode = v.get("mode").and_then(|m| m.u64()).unwrap_or(0o755) as u32;
        fs::create_dir_all(&at).map_err(|e| format!("{}: {e}", name(&rel)))?;
        let _ = fs::set_permissions(&at, fs::Permissions::from_mode(mode));
    } else if backup.is_some() || link.is_some() {
        // Made beside the path and renamed over what `apply` left, so a failure leaves that as it was.
        let tmp = parent.join(format!(".writ-back-{}-{seq}", std::process::id()));
        let done = (|| -> std::io::Result<()> {
            fs::create_dir_all(parent)?;
            if let Some(b) = backup {
                // The backup is ours, and a plain copy: the mode comes from the entry.
                let mode = v.get("mode").and_then(|m| m.u64()).unwrap_or(0o644) as u32;
                let mut from = fs::File::open(dir.join("undo").join(b))?;
                let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
                std::io::copy(&mut from, &mut f)?;
                f.sync_all()?;
                f.set_permissions(fs::Permissions::from_mode(mode))?;
            } else if let Some(l) = link {
                std::os::unix::fs::symlink(l, &tmp)?;
            }
            fs::rename(&tmp, &at)
        })();
        if let Err(e) = done {
            let _ = fs::remove_file(&tmp);
            return Err(format!("{}: {e}", name(&rel)));
        }
    } else if there.is_some() {
        fs::remove_file(&at).map_err(|e| format!("{}: {e}", name(&rel)))?;
    }
    Ok(None)
}

/// Take back what `apply` did, newest first. A path that no longer holds what `apply` left there,
/// or that something now stands in the way of, is the owner's since: it stays, and is named.
/// What is put back is recorded, so a later `apply` can tell it from the owner's change. Each entry
/// leaves undo.jsonl once it is done, so an undo that stops part way goes on from there next time.
pub fn undo(dir: &Path, real: &Path, out: &mut dyn Write) -> Result<i32, String> {
    let todo = dir.join("undo.jsonl");
    let text = fs::read_to_string(&todo).unwrap_or_default();
    // Each entry, and where its line starts, so the log can be cut back to what is still to do.
    let mut entries = Vec::new();
    let mut start = 0;
    for l in text.split_inclusive('\n') {
        entries.push((start, json::parse(l).map_err(|e| format!("{}: {e}", todo.display()))?));
        start += l.len() as u64;
    }
    if entries.is_empty() {
        let _ = writeln!(out, "nothing to undo");
        return Ok(0);
    }
    let cut = fs::OpenOptions::new().write(true).open(&todo).map_err(|e| format!("{}: {e}", todo.display()))?;
    // What is put back has a new mtime, so a build sees it changed. This log says it is the session's,
    // and says a path that stays is the owner's.
    let restored = dir.join("restored.jsonl");
    let mut log = fs::OpenOptions::new().create(true).append(true).open(&restored).map_err(|e| format!("{}: {e}", restored.display()))?;
    let mut left = Vec::new();
    let stopped = (|| -> Result<(), String> {
        for (i, (start, v)) in entries.iter().enumerate().rev() {
            let path = v.get("path").and_then(|p| p.str()).ok_or("undo.jsonl: an entry with no path")?;
            let rel = Path::new(path);
            let stays = undo_one(dir, real, i, v)
                .map_err(|e| format!("stopped at {}: {e}. What was applied after it is taken back; `writ undo` again goes on from there", name(rel)))?;
            // A path put back is recorded with what is there now, and one that stays with nothing.
            let mut pairs = vec![("path".to_string(), s(path))];
            if let Some(h) = if stays.is_none() { found_at(&real.join(rel)) } else { None } {
                pairs.push(("after".to_string(), s(&h)));
            }
            writeln!(log, "{}", Value::Obj(pairs).text()).map_err(|e| format!("{}: {e}", restored.display()))?;
            cut.set_len(*start).map_err(|e| format!("{}: {e}", todo.display()))?;
            match stays {
                Some(why) => left.push(why),
                None => {
                    let _ = writeln!(out, "restored {}", name(rel));
                }
            }
        }
        Ok(())
    })();
    for l in &left {
        let _ = writeln!(out, "{l}");
    }
    stopped?;
    let _ = fs::remove_file(&todo);
    let _ = fs::remove_dir_all(dir.join("undo"));
    Ok(if left.is_empty() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" {
        fn mknod(path: *const std::ffi::c_char, mode: u32, dev: u64) -> i32;
        fn lsetxattr(path: *const std::ffi::c_char, name: *const std::ffi::c_char, value: *const u8, size: usize, flags: i32) -> i32;
    }

    struct T {
        root: PathBuf,
        upper: PathBuf,
        real: PathBuf,
    }

    impl T {
        fn new(tag: &str) -> T {
            let root = std::env::temp_dir().join(format!("writ-gate-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let (upper, real) = (root.join("upper"), root.join("real"));
            fs::create_dir_all(&upper).unwrap();
            fs::create_dir_all(&real).unwrap();
            T { root, upper, real }
        }
        fn put(dir: &Path, rel: &str, text: &str) {
            let p = dir.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        fn real(&self, rel: &str, text: &str) {
            Self::put(&self.real, rel, text);
        }
        fn upper(&self, rel: &str, text: &str) {
            Self::put(&self.upper, rel, text);
        }
        /// A whiteout, as overlayfs leaves one: mode 0, device 0:0.
        fn whiteout(&self, rel: &str) -> bool {
            let p = self.upper.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let c = CString::new(p.as_os_str().as_bytes()).unwrap();
            let r = unsafe { mknod(c.as_ptr(), 0o020000, 0) };
            if r != 0 {
                eprintln!("no whiteout here: errno {}", std::io::Error::last_os_error());
            }
            r == 0
        }
        fn meta(&self) -> Meta {
            Meta { project: self.real.clone(), started: now() + 1000 }
        }
        fn dir(&self) -> PathBuf {
            self.root.join("session")
        }
        fn apply(&self, all: bool, named: &[&str]) -> Result<i32, String> {
            fs::create_dir_all(self.dir()).unwrap();
            let named: Vec<PathBuf> = named.iter().map(PathBuf::from).collect();
            apply(&self.upper, &self.dir(), &self.meta(), all, &named, &mut std::io::sink())
        }
        /// Run `undo`: its code, and the paths it says stay.
        fn undo(&self) -> (i32, Vec<String>) {
            let mut said = Vec::new();
            let code = undo(&self.dir(), &self.real, &mut said).unwrap();
            (code, String::from_utf8(said).unwrap().lines().filter(|l| l.ends_with(", and stays")).map(str::to_string).collect())
        }
        fn list(&self) -> Vec<(String, Kind)> {
            changes(&self.upper, &self.real).unwrap().into_iter().map(|c| (c.path.to_string_lossy().into_owned(), c.kind)).collect()
        }
        fn text(&self, rel: &str) -> String {
            fs::read_to_string(self.real.join(rel)).unwrap()
        }
    }

    impl Drop for T {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_file_the_same_as_the_real_one_is_no_change() {
        let t = T::new("same");
        t.real("a", "one");
        t.real("b", "two");
        t.upper("a", "one");
        t.upper("b", "changed");
        t.upper("c", "new");
        assert_eq!(t.list(), [("b".to_string(), Kind::File), ("c".to_string(), Kind::File)]);
    }

    #[test]
    fn git_history_in_the_box_stays_in_the_box() {
        let t = T::new("git");
        t.upper(".git/config", "[core]\nfsmonitor = /tmp/evil");
        t.upper(".git/hooks/pre-commit", "evil");
        t.upper("a", "x");
        assert_eq!(t.list(), [("a".to_string(), Kind::File)]);
    }

    #[test]
    fn what_the_real_trees_git_ignores_is_left_out_directories_too() {
        // The build's sandbox has no git.
        if !Path::new("/bin/git").exists() {
            return;
        }
        let t = T::new("ignored");
        assert!(Command::new("/bin/git").args(["init", "-q"]).arg(&t.real).status().unwrap().success());
        t.real(".gitignore", "target/\n*.log\n");
        t.upper("target/debug/x", "bin");
        t.upper("run.log", "noise");
        t.upper("src/a.rs", "x");
        let names: Vec<String> = t.list().into_iter().map(|c| c.0).collect();
        assert_eq!(names, ["src", "src/a.rs"]);
        // A new .gitignore from the box does not hide anything from this.
        t.upper(".gitignore", "src/\n");
        let names: Vec<String> = t.list().into_iter().map(|c| c.0).collect();
        assert_eq!(names, [".gitignore", "src", "src/a.rs"]);
    }

    #[test]
    fn a_repository_in_a_subdirectory_stays_in_the_box_too() {
        let t = T::new("nested");
        t.upper("sub/.git/config", "[core]\nfsmonitor = /tmp/evil");
        t.upper("sub/a.rs", "x");
        let names: Vec<String> = t.list().into_iter().map(|c| c.0).collect();
        assert_eq!(names, ["sub", "sub/a.rs"]);
        t.apply(true, &[]).unwrap();
        assert!(!t.real.join("sub/.git").exists());
    }

    #[test]
    fn what_cannot_be_read_waits_compiled_python_for_one() {
        let t = T::new("binary");
        t.upper("src/__pycache__/m.cpython-315.pyc", "\u{0}\u{0}code");
        t.upper("src/m.py", "print(1)\n");
        let list = changes(&t.upper, &t.real).unwrap();
        let pyc = list.iter().find(|c| c.path.ends_with("m.cpython-315.pyc")).unwrap();
        assert!(pyc.flags.contains(&"binary"));
        t.apply(false, &[]).unwrap();
        assert_eq!(t.text("src/m.py"), "print(1)\n");
        assert!(!t.real.join("src/__pycache__/m.cpython-315.pyc").exists());
    }

    #[test]
    fn with_no_repository_build_output_is_left_out() {
        let t = T::new("target");
        t.upper("target/debug/x", "bin");
        t.upper("src/a.rs", "x");
        let names: Vec<String> = t.list().into_iter().map(|c| c.0).collect();
        assert!(names.contains(&"src/a.rs".to_string()) && !names.iter().any(|n| n.contains("target/debug")), "{names:?}");
    }

    #[test]
    fn whiteouts_delete_and_a_directory_goes_after_what_it_holds() {
        let t = T::new("whiteout");
        t.real("gone/in/c", "c");
        t.real("gone/d", "d");
        t.real("f", "f");
        if !t.whiteout("gone") || !t.whiteout("f") {
            return;
        }
        t.whiteout("never-there");
        let l = t.list();
        let names: Vec<&str> = l.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(names, ["f", "gone/d", "gone/in/c", "gone/in", "gone"]);
        t.apply(false, &[]).unwrap();
        assert!(!t.real.join("gone").exists() && !t.real.join("f").exists());
        undo(&t.dir(), &t.real, &mut std::io::sink()).unwrap();
        assert_eq!(t.text("gone/in/c"), "c");
        assert_eq!(t.text("f"), "f");
    }

    #[test]
    fn an_opaque_directory_replaces_the_real_one_whole() {
        let t = T::new("opaque");
        t.real("keep/old", "old");
        t.upper("keep/new", "new");
        let c = CString::new(t.upper.join("keep").as_os_str().as_bytes()).unwrap();
        let set = unsafe { lsetxattr(c.as_ptr(), c"user.overlay.opaque".as_ptr(), b"y".as_ptr(), 1, 0) };
        if set != 0 {
            eprintln!("no xattrs here");
            return;
        }
        t.apply(true, &[]).unwrap();
        assert!(!t.real.join("keep/old").exists());
        assert_eq!(t.text("keep/new"), "new");
    }

    #[test]
    fn what_runs_later_waits_for_a_name_or_all() {
        let t = T::new("flag");
        t.real("Cargo.toml", "old");
        t.upper("Cargo.toml", "new");
        t.upper("build.rs", "evil");
        t.upper("x/run.sh", "evil");
        t.upper("src/a.rs", "fine");
        t.apply(false, &[]).unwrap();
        assert_eq!(t.text("src/a.rs"), "fine");
        assert_eq!(t.text("Cargo.toml"), "old");
        assert!(!t.real.join("build.rs").exists() && !t.real.join("x/run.sh").exists());
        t.apply(false, &["Cargo.toml"]).unwrap();
        assert_eq!(t.text("Cargo.toml"), "new");
        assert!(!t.real.join("build.rs").exists());
        t.apply(true, &[]).unwrap();
        assert_eq!(t.text("build.rs"), "evil");
    }

    #[test]
    fn a_symlink_waits_and_is_not_followed_when_read() {
        let t = T::new("link");
        t.real("secret-outside", "secret");
        std::os::unix::fs::symlink(t.real.join("secret-outside"), t.upper.join("evil")).unwrap();
        assert!(matches!(t.list()[0].1, Kind::Link(_)));
        t.apply(false, &[]).unwrap();
        assert!(fs::symlink_metadata(t.real.join("evil")).is_err());
        assert!(read_plain(&t.upper.join("evil")).is_err());
    }

    #[test]
    fn setuid_never_comes_out_of_a_box() {
        let t = T::new("setuid");
        t.upper("a", "x");
        fs::set_permissions(t.upper.join("a"), fs::Permissions::from_mode(0o4755)).unwrap();
        assert!(changes(&t.upper, &t.real).unwrap()[0].flags.contains(&"its mode changed"));
        t.apply(true, &[]).unwrap();
        assert_eq!(fs::metadata(t.real.join("a")).unwrap().mode() & 0o7777, 0o755);
    }

    #[test]
    fn a_fifo_is_named_never_opened_and_stops_an_apply_that_asks_for_it() {
        let t = T::new("fifo");
        assert!(Command::new("/bin/mkfifo").arg(t.upper.join("p")).status().unwrap().success());
        t.upper("a", "x");
        assert_eq!(t.list()[1].1, Kind::Unusual("a FIFO"));
        assert!(t.apply(true, &[]).unwrap_err().contains("never applied"));
        assert!(!t.real.join("a").exists(), "nothing was applied");
        t.apply(false, &[]).unwrap();
        assert_eq!(t.text("a"), "x");
    }

    #[test]
    fn a_real_symlink_in_the_way_is_not_written_through() {
        let t = T::new("through");
        let outside = t.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, t.real.join("a")).unwrap();
        t.upper("a/b", "evil");
        let e = t.apply(true, &[]).unwrap_err();
        assert!(e.contains("not a directory"), "{e}");
        assert!(!outside.join("b").exists());
    }

    #[test]
    fn the_owners_change_is_not_overwritten() {
        let t = T::new("moved");
        t.real("a", "owner's");
        t.upper("a", "session's");
        t.upper("b", "fine");
        fs::create_dir_all(t.dir()).unwrap();
        let meta = Meta { project: t.real.clone(), started: 0 };
        let e = apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink()).unwrap_err();
        assert!(e.contains("a changed in your tree"), "{e}");
        assert_eq!(t.text("a"), "owner's");
        assert!(!t.real.join("b").exists(), "nothing was applied");
    }

    #[test]
    fn a_second_apply_is_not_mistaken_for_the_owner() {
        let t = T::new("twice");
        t.upper("a", "one");
        fs::create_dir_all(t.dir()).unwrap();
        let meta = Meta { project: t.real.clone(), started: 0 };
        apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink()).unwrap();
        t.upper("a", "two");
        apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink()).unwrap();
        assert_eq!(t.text("a"), "two");
        // The owner edits it now: the session's next change must stop.
        t.real("a", "owner's");
        t.upper("a", "three");
        assert!(apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink()).is_err());
    }

    #[test]
    fn a_second_apply_over_a_link_the_session_wrote_is_not_mistaken_for_the_owner() {
        let t = T::new("relink");
        let point = |dir: &Path, to: &str| {
            let _ = fs::remove_file(dir.join("l"));
            std::os::unix::fs::symlink(to, dir.join("l")).unwrap();
        };
        point(&t.upper, "one");
        fs::create_dir_all(t.dir()).unwrap();
        let meta = Meta { project: t.real.clone(), started: 0 };
        let apply_all = || apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink());
        apply_all().unwrap();
        point(&t.upper, "two");
        apply_all().unwrap();
        assert_eq!(fs::read_link(t.real.join("l")).unwrap(), Path::new("two"));
        // The owner points it elsewhere now: the session's next change must stop.
        point(&t.real, "owner's");
        point(&t.upper, "three");
        let e = apply_all().unwrap_err();
        assert!(e.contains("l changed in your tree"), "{e}");
        assert_eq!(fs::read_link(t.real.join("l")).unwrap(), Path::new("owner's"));
    }

    #[test]
    fn undo_puts_back_what_apply_changed_and_leaves_what_the_owner_changed() {
        let t = T::new("undo");
        t.real("a", "was a");
        t.real("b", "was b");
        fs::set_permissions(t.real.join("a"), fs::Permissions::from_mode(0o750)).unwrap();
        t.upper("a", "now a");
        t.upper("b", "now b");
        t.upper("new/dir/c", "c");
        t.apply(true, &[]).unwrap();
        assert_eq!(t.text("new/dir/c"), "c");
        t.real("b", "owner's b");
        assert_eq!(undo(&t.dir(), &t.real, &mut std::io::sink()).unwrap(), 1);
        assert_eq!(t.text("a"), "was a");
        assert_eq!(fs::metadata(t.real.join("a")).unwrap().mode() & 0o777, 0o750);
        assert_eq!(t.text("b"), "owner's b");
        assert!(!t.real.join("new").exists());
        assert_eq!(undo(&t.dir(), &t.real, &mut std::io::sink()).unwrap(), 0);
    }

    #[test]
    fn undo_leaves_a_link_or_file_the_owner_replaced_after_apply_and_puts_back_the_rest() {
        let t = T::new("unlink");
        t.real("a", "was a");
        t.real("b", "was b");
        std::os::unix::fs::symlink("a", t.real.join("l")).unwrap();
        std::os::unix::fs::symlink("b", t.real.join("m")).unwrap();
        t.upper("a", "now a");
        t.upper("b", "now b");
        std::os::unix::fs::symlink("b", t.upper.join("l")).unwrap();
        std::os::unix::fs::symlink("a", t.upper.join("m")).unwrap();
        t.apply(true, &[]).unwrap();
        // The owner points the session's link elsewhere, and puts a link where its file was.
        fs::remove_file(t.real.join("l")).unwrap();
        std::os::unix::fs::symlink("owner's", t.real.join("l")).unwrap();
        fs::remove_file(t.real.join("a")).unwrap();
        std::os::unix::fs::symlink("owner's", t.real.join("a")).unwrap();
        let (code, stays) = t.undo();
        assert_eq!(code, 1);
        assert_eq!(stays, ["l changed after it was applied, and stays", "a changed after it was applied, and stays"]);
        assert_eq!(fs::read_link(t.real.join("l")).unwrap(), Path::new("owner's"));
        assert_eq!(fs::read_link(t.real.join("a")).unwrap(), Path::new("owner's"));
        assert_eq!(t.text("b"), "was b");
        assert_eq!(fs::read_link(t.real.join("m")).unwrap(), Path::new("b"));
    }

    #[test]
    fn undo_leaves_what_the_owner_put_where_apply_deleted() {
        let t = T::new("undelete");
        t.real("d/x", "was x");
        t.real("f", "was f");
        t.real("g", "was g");
        std::os::unix::fs::symlink("f", t.real.join("l")).unwrap();
        if !t.whiteout("d") || !t.whiteout("f") || !t.whiteout("g") || !t.whiteout("l") {
            return;
        }
        t.apply(true, &[]).unwrap();
        // The owner writes a file where the session deleted one, and one where it deleted a
        // directory, and a link where it deleted a link.
        t.real("f", "owner's f");
        t.real("d", "owner's d");
        std::os::unix::fs::symlink("owner's", t.real.join("l")).unwrap();
        let (code, stays) = t.undo();
        assert_eq!(code, 1);
        assert_eq!(stays.len(), 4, "{stays:?}");
        assert_eq!(stays[..3], ["l changed after it was applied, and stays", "f changed after it was applied, and stays", "d changed after it was applied, and stays"]);
        assert!(stays[3].starts_with("d/x goes through") && stays[3].ends_with("which is not a directory, and stays"), "{}", stays[3]);
        assert_eq!(t.text("f"), "owner's f");
        assert_eq!(t.text("d"), "owner's d");
        assert_eq!(fs::read_link(t.real.join("l")).unwrap(), Path::new("owner's"));
        assert_eq!(t.text("g"), "was g");
    }

    #[test]
    fn undo_leaves_a_directory_the_owner_put_where_apply_left_nothing_and_goes_on() {
        let t = T::new("redir");
        t.real("d/x", "was x");
        t.real("f", "was f");
        t.real("g", "was g");
        std::os::unix::fs::symlink("g", t.real.join("l")).unwrap();
        if !t.whiteout("d") || !t.whiteout("f") || !t.whiteout("g") || !t.whiteout("l") {
            return;
        }
        t.apply(true, &[]).unwrap();
        // The owner makes a directory where the session deleted a file, a link and a directory.
        t.real("f/mine", "owner's");
        fs::create_dir(t.real.join("l")).unwrap();
        fs::create_dir(t.real.join("d")).unwrap();
        fs::set_permissions(t.real.join("d"), fs::Permissions::from_mode(0o700)).unwrap();
        let (code, stays) = t.undo();
        assert_eq!(code, 1);
        assert_eq!(stays, ["l changed after it was applied, and stays", "f changed after it was applied, and stays", "d changed after it was applied, and stays"]);
        assert!(fs::symlink_metadata(t.real.join("l")).unwrap().is_dir());
        assert_eq!(t.text("f/mine"), "owner's");
        assert_eq!(fs::metadata(t.real.join("d")).unwrap().mode() & 0o777, 0o700);
        // What is older than the owner's directories is put back, and nothing is left to undo.
        assert_eq!(t.text("g"), "was g");
        assert_eq!(t.text("d/x"), "was x");
        assert_eq!(t.undo(), (0, vec![]));
    }

    #[test]
    fn an_undo_that_stops_part_way_goes_on_from_there_and_its_records_hold() {
        let t = T::new("partway");
        for p in ["a", "b", "c"] {
            t.real(p, &format!("was {p}"));
            t.upper(p, &format!("now {p}"));
        }
        t.apply(true, &[]).unwrap();
        t.real("c", "owner's c");
        // With a's copy gone, undo stops at a, after c and b, which were applied after it.
        let entry = read_log(&t.dir(), "undo.jsonl").unwrap().into_iter().find(|v| v.get("path").and_then(|p| p.str()) == Some("a")).unwrap();
        let copy = t.dir().join("undo").join(entry.get("backup").and_then(|b| b.str()).unwrap());
        let aside = t.root.join("aside");
        fs::rename(&copy, &aside).unwrap();
        let mut said = Vec::new();
        let e = undo(&t.dir(), &t.real, &mut said).unwrap_err();
        assert!(e.starts_with("stopped at a: "), "{e}");
        assert_eq!(String::from_utf8(said).unwrap(), "restored b\nc changed after it was applied, and stays\n");
        assert_eq!(t.text("a"), "now a");
        assert_eq!(t.text("b"), "was b");
        // To the next apply, what undo put back is the session's, and what it left is the owner's.
        let meta = Meta { project: t.real.clone(), started: 0 };
        let e = apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink()).unwrap_err();
        assert!(e.contains("c changed in your tree") && !e.contains("b changed"), "{e}");
        // The next undo goes on from a, and does not offer again what the first one did.
        fs::rename(&aside, &copy).unwrap();
        assert_eq!(t.undo(), (0, vec![]));
        assert_eq!(t.text("a"), "was a");
        assert_eq!(t.text("b"), "was b");
        assert_eq!(t.text("c"), "owner's c");
    }

    #[test]
    fn what_undo_put_back_is_not_mistaken_for_the_owner() {
        let t = T::new("again");
        t.real("a", "was a");
        std::os::unix::fs::symlink("a", t.real.join("l")).unwrap();
        t.upper("a", "now a");
        t.upper("l", "a file now");
        t.apply(true, &[]).unwrap();
        assert_eq!(undo(&t.dir(), &t.real, &mut std::io::sink()).unwrap(), 0);
        assert_eq!(t.text("a"), "was a");
        assert_eq!(fs::read_link(t.real.join("l")).unwrap(), Path::new("a"));
        // From here every mtime is since the session began, so only what the session recorded
        // tells what undo put back from the owner's change.
        let meta = Meta { project: t.real.clone(), started: 0 };
        let apply_all = || apply(&t.upper, &t.dir(), &meta, true, &[], &mut std::io::sink());
        apply_all().unwrap();
        assert_eq!(t.text("a"), "now a");
        assert_eq!(t.text("l"), "a file now");
        // The owner edits what undo put back: that is theirs, and it stops the next apply.
        assert_eq!(undo(&t.dir(), &t.real, &mut std::io::sink()).unwrap(), 0);
        t.real("a", "owner's");
        let e = apply_all().unwrap_err();
        assert!(e.contains("a changed in your tree") && !e.contains("l changed"), "{e}");
        assert_eq!(t.text("a"), "owner's");
    }

    #[test]
    fn a_name_the_session_does_not_change_is_an_error() {
        let t = T::new("named");
        t.upper("a", "x");
        assert!(t.apply(false, &["nope"]).unwrap_err().contains("does not change"));
    }

    #[test]
    fn diff_shows_changes_with_context_and_cleans_what_it_shows() {
        let before = (1..=20).map(|i| format!("line {i}\n")).collect::<String>();
        let after = before.replace("line 10\n", "line ten\n").replace("line 11\n", "");
        let d = unified(&before, &after);
        assert!(d.starts_with("@@ -7,8 +7,7 @@\n line 7\n"), "{d}");
        assert!(d.contains("-line 10\n") && d.contains("+line ten\n") && d.contains("-line 11\n"));
        assert!(!d.contains("line 3\n"));
        assert_eq!(unified("same\n", "same\n"), "");
        assert_eq!(unified("", "new\n"), "@@ -1,0 +1,1 @@\n+new\n");
        let evil = unified("a\n", "\u{1b}[2Jgotcha\n");
        assert!(!evil.contains('\u{1b}'), "{evil:?}");
    }

    #[test]
    fn two_changes_far_apart_are_two_hunks() {
        let before = (1..=40).map(|i| format!("{i}\n")).collect::<String>();
        let after = before.replace("\n5\n", "\nfive\n").replace("\n35\n", "\nthirty-five\n");
        let d = unified(&before, &after);
        assert_eq!(d.matches("@@ -").count(), 2, "{d}");
        assert!(d.contains("@@ -32,7 +32,7 @@"), "{d}");
    }

    #[test]
    fn a_long_file_compares_as_one_replacement_rather_than_running_out_of_memory() {
        let a: Vec<String> = (0..MAX_DIFF_LINES + 10).map(|i| format!("a{i}")).collect();
        let b: Vec<String> = (0..MAX_DIFF_LINES + 10).map(|i| format!("b{i}")).collect();
        let (x, y): (Vec<&str>, Vec<&str>) = (a.iter().map(|s| s.as_str()).collect(), b.iter().map(|s| s.as_str()).collect());
        assert_eq!(line_ops(&x, &y).len(), 2 * (MAX_DIFF_LINES + 10));
    }

    #[test]
    fn what_runs_later_is_named() {
        for p in ["build.rs", "a/b/build.rs", "Cargo.toml", "x/Makefile.am", "configure", "a.sh", ".cargo/config.toml", ".github/workflows/x.yml", "AGENTS.md", ".claude/settings.json", "package.json", "go.mod", ".envrc", "pkgs/x/pkg.toml", "crates/foo/.cargo/config.toml", "sub/.vscode/tasks.json", "tests/conftest.py", "x.pth"] {
            assert!(runs_later(Path::new(p)), "{p}");
        }
        for p in ["src/main.rs", "README.md", "src/lib.go"] {
            assert!(!runs_later(Path::new(p)), "{p}");
        }
    }
}
