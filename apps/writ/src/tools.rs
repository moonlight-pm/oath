//! The tools, on the box's side. A call comes in on stdin as one line of JSON and its answer goes
//! back on stdout. No path is checked here: the box decides what a path can reach, so this code
//! has nothing to get wrong about that. The host reads the answers as data.

use crate::chat::Tool;
use crate::json::{self, n, obj, s, Value};
use crate::{inner, sys};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The longest call the host sends: a write of the largest arguments a turn may hold.
pub const MAX_CALL: usize = 9 << 20;
/// The longest answer the host accepts. Every tool here caps its own below this.
pub const MAX_ANSWER: usize = 1 << 20;

const MAX_READ_LINES: u64 = 2000;
const MAX_READ_BYTES: usize = 64 << 10;
/// Larger than this and a file is not read at all.
const MAX_FILE: u64 = 64 << 20;
const OUTPUT_HEAD: usize = 8 << 10;
const OUTPUT_TAIL: usize = 8 << 10;
/// What is kept of a command's output for reading later, in the box's `/tmp`.
const MAX_SPILL: u64 = 256 << 20;
const SHELL_DEFAULT_SECS: u64 = 120;
pub const SHELL_MAX_SECS: u64 = 600;
const SEARCH_SECS: u64 = 60;
const MAX_FOUND: usize = 500;
const MAX_GREP_LINES: usize = 200;
/// Directories a search does not walk unless asked to.
const NOT_SEARCHED: &[&str] = &[".git", "target", "node_modules"];

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "read",
        description: "Read a text file, with line numbers. Long files come in windows: the answer says the next offset. You must read a file before you edit or overwrite it.",
        parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"The file, relative to the project."},"offset":{"type":"integer","description":"The first line to read, from 1."},"limit":{"type":"integer","description":"How many lines. At most 2000."}},"required":["path"]}"#,
    },
    Tool {
        name: "edit",
        description: "Replace one exact piece of a file. `old` must appear exactly once: copy it from the file without its line numbers, with enough around it to be unique.",
        parameters: r#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string","description":"The text to replace."},"new":{"type":"string","description":"What to put there."}},"required":["path","old","new"]}"#,
    },
    Tool {
        name: "write",
        description: "Create a file, or replace one you have read. Parent directories are made.",
        parameters: r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
    },
    Tool {
        name: "grep",
        description: "Search file contents with an extended regular expression. Gives file:line:text, skipping .git, target and node_modules.",
        parameters: r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"A file or directory. Default: the project."}},"required":["pattern"]}"#,
    },
    Tool {
        name: "glob",
        description: "Find files by name, with * and ? within a name and ** across directories. A pattern with no slash matches at any depth.",
        parameters: r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"Where to look. Default: the project."}},"required":["pattern"]}"#,
    },
    Tool {
        name: "shell",
        description: "Run a command with /bin/sh in the project. There is no network and no terminal, so nothing can prompt. Gives the exit code and output; long output keeps its start and end, and the answer names a file with the rest.",
        parameters: r#"{"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer","description":"Seconds. Default 120, at most 600."}},"required":["command"]}"#,
    },
];

fn hash_of(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn text_arg<'a>(a: &'a Value, key: &str) -> Result<&'a str, String> {
    a.get(key).and_then(|v| v.str()).ok_or(format!("this tool needs {key}, a string"))
}

fn number_arg(a: &Value, key: &str) -> Result<Option<u64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.u64().map(Some).ok_or(format!("{key} is a whole number")),
    }
}

/// What the tools remember between calls.
#[derive(Default)]
pub struct State {
    /// What each file held when last read or written, so an edit cannot act on a file it has not
    /// seen as it is now.
    seen: HashMap<PathBuf, u64>,
    spills: u32,
}

impl State {
    fn key(path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    /// A file an edit or write may change: read since it last changed, or not there.
    fn current(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        let m = match fs::symlink_metadata(path) {
            Err(_) => return Ok(None),
            Ok(m) => m,
        };
        if !fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
            let kind = if m.file_type().is_symlink() { "a dangling symlink" } else { "not a regular file" };
            return Err(format!("{} is {kind}", path.display()));
        }
        if m.len() > MAX_FILE {
            return Err(format!("{} is over {MAX_FILE} bytes", path.display()));
        }
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        match self.seen.get(&Self::key(path)) {
            None => Err(format!("read {} first", path.display())),
            Some(h) if *h != hash_of(&bytes) => Err(format!("{} changed since you read it: read it again", path.display())),
            Some(_) => Ok(Some(bytes)),
        }
    }

    pub fn read(&mut self, a: &Value) -> Result<String, String> {
        let path = PathBuf::from(text_arg(a, "path")?);
        let offset = number_arg(a, "offset")?.unwrap_or(1).max(1);
        let limit = number_arg(a, "limit")?.unwrap_or(MAX_READ_LINES).clamp(1, MAX_READ_LINES);
        // Not a FIFO or a device: reading one would hang or never end.
        let m = fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if m.is_dir() {
            return Err(format!("{} is a directory: use glob", path.display()));
        }
        if !m.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
        if m.len() > MAX_FILE {
            return Err(format!("{} is {} bytes: use grep or shell", path.display(), m.len()));
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .and_then(|f| f.take(MAX_FILE).read_to_end(&mut bytes))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.iter().take(8192).any(|b| *b == 0) {
            return Err(format!("{} is binary, {} bytes", path.display(), bytes.len()));
        }
        self.seen.insert(Self::key(&path), hash_of(&bytes));
        let text = String::from_utf8_lossy(&bytes);
        let mut out = String::new();
        let mut line_no = 0u64;
        let mut shown = 0u64;
        let mut more = false;
        for line in text.split_inclusive('\n') {
            line_no += 1;
            if line_no < offset {
                continue;
            }
            if shown >= limit || out.len() + line.len() > MAX_READ_BYTES {
                more = true;
                break;
            }
            let line = line.strip_suffix('\n').unwrap_or(line);
            out.push_str(&format!("{line_no}\t{line}\n"));
            shown += 1;
        }
        if text.is_empty() {
            return Ok(format!("{} is empty", path.display()));
        }
        if shown == 0 && !more {
            return Ok(format!("{} has {line_no} lines, so nothing is at line {offset}", path.display()));
        }
        if more {
            out.push_str(&format!("[more: read again with offset {}]\n", offset + shown));
        }
        Ok(out)
    }

    pub fn write(&mut self, a: &Value) -> Result<String, String> {
        let path = PathBuf::from(text_arg(a, "path")?);
        let content = text_arg(a, "content")?;
        let before = self.current(&path)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        put(&path, content.as_bytes())?;
        self.seen.insert(Self::key(&path), hash_of(content.as_bytes()));
        let verb = if before.is_some() { "replaced" } else { "created" };
        Ok(format!("{verb} {}, {} bytes", path.display(), content.len()))
    }

    pub fn edit(&mut self, a: &Value) -> Result<String, String> {
        let path = PathBuf::from(text_arg(a, "path")?);
        let (old, new) = (text_arg(a, "old")?, text_arg(a, "new")?);
        if old.is_empty() {
            return Err("old is empty: to create a file use write".into());
        }
        let Some(bytes) = self.current(&path)? else {
            return Err(format!("{} does not exist", path.display()));
        };
        let text = String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", path.display()))?;
        let (after, how) = replace(&text, old, new).map_err(|e| format!("{}: {e}", path.display()))?;
        put(&path, after.as_bytes())?;
        self.seen.insert(Self::key(&path), hash_of(after.as_bytes()));
        let lines = |t: &str| t.lines().count();
        Ok(format!("edited {}{how}: {} lines became {}", path.display(), lines(old), lines(new)))
    }

    pub fn grep(&mut self, a: &Value) -> Result<String, String> {
        let pattern = text_arg(a, "pattern")?;
        let path = a.get("path").and_then(|p| p.str()).unwrap_or(".");
        let mut c = Command::new("/bin/grep");
        c.args(["-rnIE", "--color=never"]);
        for d in NOT_SEARCHED {
            c.arg(format!("--exclude-dir={d}"));
        }
        c.arg("-e").arg(pattern).arg("--").arg(path);
        let out = capture(c, SEARCH_SECS, &mut self.spills)?;
        if out.timed_out {
            return Err("grep ran out of time: narrow the path".into());
        }
        // 1 is grep's way of saying nothing matched, and 2 is a real error.
        match out.code {
            Some(1) => return Ok("no match".into()),
            Some(0) => {}
            _ => return Err(format!("grep failed: {}", out.text.trim())),
        }
        let lines: Vec<&str> = out.text.lines().collect();
        let mut shown: String = lines.iter().take(MAX_GREP_LINES).map(|l| {
            let mut l = l.to_string();
            if l.len() > 400 {
                let end = (0..=400).rev().find(|i| l.is_char_boundary(*i)).unwrap_or(0);
                l.truncate(end);
                l.push_str(" [line cut]");
            }
            l + "\n"
        }).collect();
        if lines.len() > MAX_GREP_LINES {
            shown.push_str(&format!("[{} more lines: narrow the pattern or the path]\n", lines.len() - MAX_GREP_LINES));
        }
        Ok(shown)
    }

    pub fn glob(&mut self, a: &Value) -> Result<String, String> {
        let pattern = text_arg(a, "pattern")?;
        if pattern.len() > 256 || pattern.matches('*').count() > 12 {
            return Err("that pattern is too long".into());
        }
        let root = PathBuf::from(a.get("path").and_then(|p| p.str()).unwrap_or("."));
        // No slash: a name, at any depth.
        let full = if pattern.contains('/') { pattern.to_string() } else { format!("**/{pattern}") };
        let (mut found, mut more) = (Vec::new(), false);
        let deadline = Instant::now() + Duration::from_secs(SEARCH_SECS);
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if Instant::now() > deadline {
                return Err("glob ran out of time: narrow the path".into());
            }
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                let Ok(kind) = e.file_type() else { continue };
                if kind.is_dir() {
                    if !NOT_SEARCHED.contains(&name.as_ref()) {
                        stack.push(e.path());
                    }
                    continue;
                }
                let path = e.path();
                let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().into_owned();
                if glob_match(full.as_bytes(), rel.as_bytes()) {
                    if found.len() >= MAX_FOUND {
                        more = true;
                    } else {
                        found.push(rel);
                    }
                }
            }
        }
        found.sort();
        let mut out = found.join("\n");
        if found.is_empty() {
            out = "no file matches".into();
        } else if more {
            out.push_str(&format!("\n[over {MAX_FOUND} files: narrow the pattern]"));
        }
        Ok(out)
    }

    pub fn shell(&mut self, a: &Value) -> Result<String, String> {
        let command = text_arg(a, "command")?;
        let secs = number_arg(a, "timeout")?.unwrap_or(SHELL_DEFAULT_SECS).clamp(1, SHELL_MAX_SECS);
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(command);
        let out = capture(c, secs, &mut self.spills)?;
        let status = match (out.timed_out, out.code) {
            (true, _) => format!("killed after {secs} seconds"),
            (false, Some(code)) => format!("exit {code}"),
            (false, None) => "killed by a signal".to_string(),
        };
        let mut text = format!("{status}\n{}", out.text);
        if out.held_open {
            text.push_str("\n[a process it left running still holds this output; what came after is not here]");
        }
        Ok(text)
    }

    /// One call from the host. A tool's refusal is an answer for the model, not a failure here.
    pub fn call(&mut self, tool: &str, args: &Value) -> Result<String, String> {
        match tool {
            "read" => self.read(args),
            "edit" => self.edit(args),
            "write" => self.write(args),
            "grep" => self.grep(args),
            "glob" => self.glob(args),
            "shell" => self.shell(args),
            "project-notes" => Ok(project_notes()),
            other => Err(format!("there is no tool {other}")),
        }
    }
}

/// What the machine and the project say about working here, for the host to label and hand to the
/// model. Read in the box, like everything else the project holds.
fn project_notes() -> String {
    let mut out = String::new();
    let mut project_said = false;
    for (label, path) in [("This machine", "/oath/INDEX.md"), ("This project", "AGENTS.md"), ("This project", "CLAUDE.md")] {
        let Ok(m) = fs::metadata(path) else { continue };
        if !m.is_file() || (project_said && label == "This project") {
            continue;
        }
        project_said = label == "This project";
        let mut bytes = Vec::new();
        if fs::File::open(path).and_then(|f| f.take(16 << 10).read_to_end(&mut bytes)).is_err() {
            continue;
        }
        out.push_str(&format!("<notes source=\"{path}\" about=\"{label}\">\n{}\n</notes>\n", String::from_utf8_lossy(&bytes)));
    }
    out
}

/// Write a file whole or not at all, keeping its mode. The temporary name sits beside it, so the
/// rename stays on one filesystem.
fn put(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mode = fs::metadata(path).map(|m| m.permissions().mode() & 0o777).ok();
    let name = path.file_name().ok_or(format!("{} has no file name", path.display()))?;
    let mut tmp = OsString::from(".writ-");
    tmp.push(std::process::id().to_string());
    tmp.push("-");
    tmp.push(name);
    let tmp = path.with_file_name(tmp);
    let write = || -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        if let Some(m) = mode {
            f.set_permissions(fs::Permissions::from_mode(m))?;
        }
        Ok(())
    };
    let done = write().and_then(|_| fs::rename(&tmp, path));
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done.map_err(|e| format!("{}: {e}", path.display()))
}

/// A character as a model tends to retype it, for the first rung of the ladder.
fn plain(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        '\u{00a0}' | '\u{2002}'..='\u{200a}' | '\u{202f}' | '\u{3000}' => ' ',
        c => c,
    }
}

/// Replace `old` in `text`, once. Exact first; then a ladder of what a model retypes wrongly, each
/// rung by whole lines: Unicode look-alikes, then trailing whitespace, then indentation. It never
/// guesses between two places, and no model is asked to fix anything. Returns what rung it took.
fn replace(text: &str, old: &str, new: &str) -> Result<(String, &'static str), String> {
    // Line endings are the file's: a model writes \n.
    let crlf = text.contains("\r\n");
    let new_text = if crlf { new.replace("\r\n", "\n").replace('\n', "\r\n") } else { new.to_string() };
    let old_text = if crlf { old.replace("\r\n", "\n").replace('\n', "\r\n") } else { old.to_string() };
    match text.matches(&old_text).count() {
        1 => return Ok((text.replacen(&old_text, &new_text, 1), "")),
        0 => {}
        k => {
            let lines: Vec<String> = text.match_indices(&old_text).take(8).map(|(at, _)| (text[..at].matches('\n').count() + 1).to_string()).collect();
            return Err(format!("old appears {k} times, at lines {}: give more around it", lines.join(", ")));
        }
    }
    // A leading or trailing newline in old says nothing about which lines it means.
    let wanted: Vec<&str> = old.trim_matches('\n').split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let bare = |l: &str| l.strip_suffix('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).unwrap_or(l).to_string();
    let unicode = |l: &str| l.chars().map(plain).collect::<String>();
    let trailing = |l: &str| unicode(l).trim_end().to_string();
    let indent = |l: &str| unicode(l).trim().to_string();
    let rungs: [(&str, &dyn Fn(&str) -> String); 3] =
        [(" (matching look-alike characters)", &unicode), (" (ignoring trailing whitespace)", &trailing), (" (ignoring indentation)", &indent)];
    for (how, norm) in rungs {
        let want: Vec<String> = wanted.iter().map(|l| norm(l)).collect();
        if want.iter().all(|l| l.is_empty()) || want.len() > lines.len() {
            continue;
        }
        let found: Vec<usize> = (0..=lines.len() - want.len())
            .filter(|at| (0..want.len()).all(|i| norm(&bare(lines[at + i])) == want[i]))
            .collect();
        match found.len() {
            0 => continue,
            1 => {
                let at = found[0];
                let mut out: String = lines[..at].concat();
                out.push_str(new_text.trim_end_matches('\n').trim_end_matches('\r'));
                // The last replaced line's own ending stays, and a replacement to nothing takes it too.
                let last = lines[at + want.len() - 1];
                if !new.trim_end_matches('\n').is_empty() {
                    out.push_str(if last.ends_with("\r\n") { "\r\n" } else if last.ends_with('\n') { "\n" } else { "" });
                }
                out.push_str(&lines[at + want.len()..].concat());
                return Ok((out, how));
            }
            k => return Err(format!("old appears {k} times{how}: give more around it")),
        }
    }
    Err("old is not in the file: read it again and copy the text exactly".into())
}

/// `*` and `?` within a name, `**/` across directories.
fn glob_match(p: &[u8], t: &[u8]) -> bool {
    match p {
        [] => t.is_empty(),
        [b'*', b'*', b'/', rest @ ..] => {
            glob_match(rest, t)
                || t.iter().enumerate().any(|(i, c)| *c == b'/' && glob_match(rest, &t[i + 1..]))
        }
        [b'*', b'*'] => true,
        [b'*', rest @ ..] => {
            let end = t.iter().position(|c| *c == b'/').unwrap_or(t.len());
            (0..=end).any(|i| glob_match(rest, &t[i..]))
        }
        [b'?', rest @ ..] => t.first().is_some_and(|c| *c != b'/') && glob_match(rest, &t[1..]),
        [c, rest @ ..] => t.first() == Some(c) && glob_match(rest, &t[1..]),
    }
}

struct Captured {
    code: Option<i32>,
    timed_out: bool,
    held_open: bool,
    text: String,
}

/// A command's output, start and end in memory, and all of it past that in a file in the box.
#[derive(Default)]
struct Output {
    head: Vec<u8>,
    tail: Vec<u8>,
    total: u64,
    spill: Option<(PathBuf, fs::File)>,
    finished: bool,
}

impl Output {
    fn push(&mut self, bytes: &[u8], spill_to: &Path) {
        self.total += bytes.len() as u64;
        if self.spill.is_none() && self.head.len() + bytes.len() > OUTPUT_HEAD + OUTPUT_TAIL {
            if let Ok(mut f) = fs::File::create(spill_to) {
                let _ = f.write_all(&self.head);
                self.spill = Some((spill_to.to_path_buf(), f));
            }
        }
        if let Some((_, f)) = &mut self.spill {
            if self.total <= MAX_SPILL {
                let _ = f.write_all(bytes);
            }
        }
        let room = (OUTPUT_HEAD + OUTPUT_TAIL).saturating_sub(self.head.len());
        let (first, rest) = bytes.split_at(bytes.len().min(room));
        self.head.extend_from_slice(first);
        if self.total > (OUTPUT_HEAD + OUTPUT_TAIL) as u64 {
            self.tail.extend_from_slice(rest);
            let over = self.tail.len().saturating_sub(OUTPUT_TAIL);
            self.tail.drain(..over);
        }
    }

    fn text(&self) -> String {
        let keep = (OUTPUT_HEAD + OUTPUT_TAIL) as u64;
        if self.total <= keep {
            return String::from_utf8_lossy(&self.head).into_owned();
        }
        let start = String::from_utf8_lossy(&self.head[..self.head.len().min(OUTPUT_HEAD)]);
        // The tail is whatever came last: the end of what head holds, if nothing passed it.
        let end_from_head = &self.head[self.head.len().min(OUTPUT_HEAD)..];
        let mut end = end_from_head.to_vec();
        end.extend_from_slice(&self.tail);
        let cut = end.len().saturating_sub(OUTPUT_TAIL);
        let end = String::from_utf8_lossy(&end[cut..]).into_owned();
        let whole = match &self.spill {
            Some((p, _)) if self.total <= MAX_SPILL => format!("all of it is in {}", p.display()),
            Some((p, _)) => format!("the first {MAX_SPILL} bytes are in {}", p.display()),
            None => "the rest was not kept".to_string(),
        };
        format!("{start}\n[{} bytes in all, with the middle left out; {whole}]\n{end}", self.total)
    }
}

/// Run a command to its end, or its time, with stdout and stderr together in the order they came.
/// It gets no stdin, and its own process group, so a timeout takes whatever it started.
fn capture(mut c: Command, secs: u64, spills: &mut u32) -> Result<Captured, String> {
    let (reader, writer) = std::io::pipe().map_err(|e| format!("pipe: {e}"))?;
    let other = writer.try_clone().map_err(|e| format!("pipe: {e}"))?;
    c.stdin(Stdio::null()).stdout(writer).stderr(other).process_group(0);
    let mut child = c.spawn().map_err(|e| format!("cannot run it: {e}"))?;
    drop(c);
    *spills += 1;
    let spill_to = std::env::temp_dir().join(format!("writ-output-{}-{spills}", std::process::id()));
    let out = Arc::new(Mutex::new(Output::default()));
    let filling = out.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 16384];
        while let Ok(k) = reader.read(&mut buf) {
            if k == 0 {
                break;
            }
            filling.lock().unwrap().push(&buf[..k], &spill_to);
        }
        filling.lock().unwrap().finished = true;
    });
    let deadline = Instant::now() + Duration::from_secs(secs);
    let (status, timed_out) = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break (st, false);
        }
        if Instant::now() > deadline {
            unsafe { sys::kill(-(child.id() as i32), sys::SIGKILL) };
            break (child.wait().map_err(|e| e.to_string())?, true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // What it started may hold the pipe open. Its output so far is what there is.
    let grace = Instant::now() + Duration::from_secs(1);
    while !out.lock().unwrap().finished && Instant::now() < grace {
        std::thread::sleep(Duration::from_millis(5));
    }
    let o = out.lock().unwrap();
    Ok(Captured { code: status.code(), timed_out, held_open: !o.finished, text: o.text() })
}

/// One answer, as one line.
pub fn answer(id: u64, result: &Result<String, String>) -> String {
    let (ok, text) = match result {
        Ok(t) => (true, t.as_str()),
        Err(e) => (false, e.as_str()),
    };
    // Anything longer would be refused whole by the host, so it comes cut, and says so.
    let text = if text.len() > MAX_ANSWER / 2 {
        let end = (0..=MAX_ANSWER / 2).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
        format!("{}\n[cut at {end} bytes]", &text[..end])
    } else {
        text.to_string()
    };
    obj([("id", n(id)), ("ok", Value::Bool(ok)), ("text", s(&text))]).text()
}

/// The server, in a box and nowhere else.
pub fn main() -> ! {
    if unsafe { sys::prctl(sys::PR_GET_SECCOMP, 0i64, 0i64, 0i64, 0i64) } != 2 {
        eprintln!("writ tools runs only in a box");
        std::process::exit(inner::REFUSED_TO_START);
    }
    // A command must not be able to open this process's pipes through /proc and answer in its name.
    unsafe { sys::prctl(sys::PR_SET_DUMPABLE, 0i64, 0i64, 0i64, 0i64) };
    let mut state = State::default();
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut stdout = std::io::stdout();
    let mut line = Vec::new();
    loop {
        line.clear();
        match stdin.by_ref().take(MAX_CALL as u64 + 1).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => std::process::exit(0),
            Ok(_) => {}
        }
        if line.len() > MAX_CALL {
            std::process::exit(inner::REFUSED_TO_START);
        }
        let parsed = std::str::from_utf8(&line).map_err(|e| e.to_string()).and_then(json::parse);
        let Ok(call) = parsed else { std::process::exit(inner::REFUSED_TO_START) };
        let id = call.get("id").and_then(|v| v.u64()).unwrap_or(0);
        let tool = call.get("tool").and_then(|v| v.str()).unwrap_or("");
        let empty = Value::Obj(Vec::new());
        let result = state.call(tool, call.get("args").unwrap_or(&empty));
        let mut reply = answer(id, &result);
        reply.push('\n');
        if stdout.write_all(reply.as_bytes()).and_then(|_| stdout.flush()).is_err() {
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("writ-tools-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn path(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn args<const N: usize>(pairs: [(&str, &str); N]) -> Value {
        Value::Obj(pairs.iter().map(|(k, v)| (k.to_string(), s(v))).collect())
    }

    #[test]
    fn replace_is_exact_and_unique_first() {
        assert_eq!(replace("a b c", "b", "x").unwrap(), ("a x c".to_string(), ""));
        let e = replace("a\nb\na\n", "a", "x").unwrap_err();
        assert!(e.contains("2 times") && e.contains("lines 1, 3"), "{e}");
        assert!(replace("abc", "z", "x").unwrap_err().contains("not in the file"));
    }

    #[test]
    fn the_ladder_finds_what_a_model_retypes_and_says_so() {
        let (t, how) = replace("let s = \u{201c}hi\u{201d};\nnext\n", "let s = \"hi\";", "let s = 1;").unwrap();
        assert_eq!(t, "let s = 1;\nnext\n");
        assert!(how.contains("look-alike"));
        let (t, how) = replace("fn a() {   \n}\n", "fn a() {\n}", "fn b() {\n}").unwrap();
        assert_eq!(t, "fn b() {\n}\n");
        assert!(how.contains("trailing"));
        let (t, how) = replace("    x = 1\n    y = 2\nz\n", "x = 1\ny = 2", "    x = 3").unwrap();
        assert_eq!(t, "    x = 3\nz\n");
        assert!(how.contains("indentation"));
    }

    #[test]
    fn the_ladder_never_guesses_between_two_places() {
        let e = replace("a = \u{201c}1\u{201d}\nb\na = \u{201c}1\u{201d}\n", "a = \"1\"", "x").unwrap_err();
        assert!(e.contains("2 times"), "{e}");
    }

    #[test]
    fn line_endings_are_the_files() {
        let (t, _) = replace("a\r\nb\r\nc\r\n", "a\nb", "x\ny").unwrap();
        assert_eq!(t, "x\r\ny\r\nc\r\n");
    }

    #[test]
    fn read_gives_numbers_and_windows() {
        let d = Dir::new("read");
        let p = d.path("f");
        fs::write(&p, (1..=5).map(|i| format!("l{i}\n")).collect::<String>()).unwrap();
        let mut st = State::default();
        assert_eq!(st.read(&args([("path", &p)])).unwrap(), "1\tl1\n2\tl2\n3\tl3\n4\tl4\n5\tl5\n");
        let a = Value::Obj(vec![("path".into(), s(&p)), ("offset".into(), n(2)), ("limit".into(), n(2))]);
        assert_eq!(st.read(&a).unwrap(), "2\tl2\n3\tl3\n[more: read again with offset 4]\n");
        let past = Value::Obj(vec![("path".into(), s(&p)), ("offset".into(), n(9))]);
        assert!(st.read(&past).unwrap().contains("nothing is at line 9"));
        fs::write(&p, b"a\0b").unwrap();
        assert!(st.read(&args([("path", &p)])).unwrap_err().contains("binary"));
    }

    #[test]
    fn read_refuses_what_would_hang() {
        let d = Dir::new("fifo");
        let mut st = State::default();
        assert!(st.read(&args([("path", "/dev/zero")])).is_err());
        assert!(st.read(&args([("path", &d.path(""))])).unwrap_err().contains("directory"));
        let fifo = d.path("f");
        assert!(Command::new("/bin/mkfifo").arg(&fifo).status().unwrap().success());
        assert!(st.read(&args([("path", &fifo)])).unwrap_err().contains("not a regular file"));
    }

    #[test]
    fn a_file_must_be_read_as_it_is_before_it_changes() {
        let d = Dir::new("stale");
        let p = d.path("f");
        fs::write(&p, "one\n").unwrap();
        let mut st = State::default();
        let edit = args([("path", &p), ("old", "one"), ("new", "two")]);
        assert!(st.edit(&edit).unwrap_err().contains("read"));
        assert!(st.write(&args([("path", &p), ("content", "x")])).unwrap_err().contains("read"));
        st.read(&args([("path", &p)])).unwrap();
        fs::write(&p, "one, changed\n").unwrap();
        assert!(st.edit(&edit).unwrap_err().contains("changed since"));
        st.read(&args([("path", &p)])).unwrap();
        st.edit(&edit).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two, changed\n");
        // Its own edit does not make it stale.
        st.edit(&args([("path", &p), ("old", "two"), ("new", "three")])).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "three, changed\n");
    }

    #[test]
    fn write_makes_parents_keeps_the_mode_and_leaves_nothing_beside() {
        let d = Dir::new("write");
        let p = d.path("a/b/f.sh");
        let mut st = State::default();
        st.write(&args([("path", &p), ("content", "one")])).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        st.read(&args([("path", &p)])).unwrap();
        st.write(&args([("path", &p), ("content", "two")])).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two");
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read_dir(d.path("a/b")).unwrap().count(), 1);
    }

    #[test]
    fn glob_matches_as_it_says() {
        for (p, t, want) in [
            ("**/*.rs", "src/main.rs", true),
            ("**/*.rs", "main.rs", true),
            ("src/*.rs", "src/a/main.rs", false),
            ("src/**/*.rs", "src/a/b/main.rs", true),
            ("src/**", "src/a/b", true),
            ("?.rs", "a.rs", true),
            ("?.rs", "ab.rs", false),
            ("*.rs", "a/b.rs", false),
        ] {
            assert_eq!(glob_match(p.as_bytes(), t.as_bytes()), want, "{p} {t}");
        }
    }

    #[test]
    fn glob_and_grep_find_files_and_skip_build_output() {
        let d = Dir::new("search");
        fs::create_dir_all(d.path("src")).unwrap();
        fs::create_dir_all(d.path("target")).unwrap();
        fs::write(d.path("src/a.rs"), "needle one\n").unwrap();
        fs::write(d.path("target/b.rs"), "needle two\n").unwrap();
        let mut st = State::default();
        let found = st.glob(&args([("pattern", "*.rs"), ("path", &d.0.to_string_lossy())])).unwrap();
        assert_eq!(found, "src/a.rs");
        let hits = st.grep(&args([("pattern", "needle"), ("path", &d.0.to_string_lossy())])).unwrap();
        assert!(hits.contains("a.rs:1:needle one") && !hits.contains("two"), "{hits}");
        assert_eq!(st.grep(&args([("pattern", "absent"), ("path", &d.0.to_string_lossy())])).unwrap(), "no match");
        assert!(st.grep(&args([("pattern", "(")])).is_err());
    }

    #[test]
    fn shell_gives_the_code_and_both_streams_in_order() {
        let mut st = State::default();
        let t = st.shell(&args([("command", "echo one; echo two >&2; echo three; exit 3")])).unwrap();
        assert_eq!(t, "exit 3\none\ntwo\nthree\n");
    }

    #[test]
    fn long_output_keeps_its_ends_and_names_the_rest() {
        let mut st = State::default();
        let t = st.shell(&args([("command", "echo FIRST; head -c 200000 /dev/zero | tr '\\0' x; echo; echo LAST")])).unwrap();
        assert!(t.contains("FIRST") && t.trim_end().ends_with("LAST"));
        assert!(t.len() < OUTPUT_HEAD + OUTPUT_TAIL + 200, "{}", t.len());
        let name = t.split("all of it is in ").nth(1).unwrap().split(']').next().unwrap();
        let all = fs::read_to_string(name).unwrap();
        assert!(all.len() > 200_000 && all.starts_with("FIRST") && all.trim_end().ends_with("LAST"));
        fs::remove_file(name).unwrap();
    }

    #[test]
    fn a_timeout_takes_what_the_command_started() {
        let mut st = State::default();
        let a = Value::Obj(vec![("command".into(), s("sleep 30 & echo $!; wait")), ("timeout".into(), n(1))]);
        let start = Instant::now();
        let t = st.shell(&a).unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(t.starts_with("killed after 1 seconds"), "{t}");
        let pid = t.lines().nth(1).unwrap().trim();
        std::thread::sleep(Duration::from_millis(100));
        let state = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        assert!(state.is_empty() || state.contains(") Z"), "still running: {state}");
    }

    #[test]
    fn a_process_left_running_does_not_hold_the_answer() {
        let mut st = State::default();
        let start = Instant::now();
        let t = st.shell(&args([("command", "echo here; sleep 5 &")])).unwrap();
        assert!(start.elapsed() < Duration::from_secs(3), "{:?}", start.elapsed());
        assert!(t.starts_with("exit 0\nhere\n") && t.contains("still holds"), "{t}");
    }

    #[test]
    fn an_answer_is_one_line_and_bounded() {
        let long = "é\n".repeat(MAX_ANSWER);
        let a = answer(7, &Ok(long));
        assert!(a.len() <= MAX_ANSWER + 1024 && !a.contains('\n'));
        let v = json::parse(&a).unwrap();
        assert_eq!(v.get("id").and_then(|i| i.u64()), Some(7));
        assert_eq!(v.get("ok"), Some(&Value::Bool(true)));
        let e = json::parse(&answer(8, &Err("no".into()))).unwrap();
        assert_eq!(e.get("ok"), Some(&Value::Bool(false)));
    }

    #[test]
    fn a_tool_with_bad_arguments_is_an_answer_not_a_panic() {
        let mut st = State::default();
        for (tool, a) in [
            ("read", obj([])),
            ("read", obj([("path", n(3))])),
            ("edit", obj([("path", s("/x"))])),
            ("write", obj([("content", s("x"))])),
            ("grep", obj([])),
            ("glob", obj([("pattern", s(&"*".repeat(99)))])),
            ("shell", obj([("command", Value::Null)])),
            ("shell", obj([("command", s("true")), ("timeout", s("soon"))])),
            ("nothing", obj([])),
        ] {
            assert!(st.call(tool, &a).is_err(), "{tool}");
        }
    }
}
