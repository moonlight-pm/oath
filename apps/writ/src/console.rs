//! `writ` on a terminal: a session you stay in. You type, the model works in its box, you read what
//! it held, and you apply it, all in one place, in the terminal's own scrollback.
//!
//! Three threads. One reads keys. One does the work: the model, the box, the gate, one job at a time,
//! so `/apply` cannot run beside a model. This one holds the screen, and is the only one that writes
//! to the terminal. The rules that matter are elsewhere and still hold: every byte passes
//! `screen.rs`, the box is down whenever the prompt is the person's (`agent::Conversation`), and what
//! decides anything comes from the keyboard, since nothing a model says is ever read as a command.

use crate::agent::{self, Conversation, End, Report};
use crate::chat::{Delta, Usage};
use crate::editor::{Decoder, Edit, Editor, Key};
use crate::json::Value;
use crate::model::Cancel;
use crate::screen::{self, Row, Screen, Style};
use crate::{gate, landlock, serve, sys, term};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How often the status row moves while something is running.
const TICK: Duration = Duration::from_millis(100);
/// How often an idle session looks at the terminal's size.
const IDLE_TICK: Duration = Duration::from_millis(250);
/// Text arrives faster than it is worth drawing.
const FRAME: Duration = Duration::from_millis(33);
/// How long a lone escape waits to see if it starts a sequence. Terminals send those whole.
const ESCAPE_WAIT_MS: i32 = 30;
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const FAILED_LINES_SHOWN: usize = 3;
const QUEUED_SHOWN: usize = 3;

const HELP: &str = "\
Enter sends. Alt-Enter, Ctrl-J or a backslash before Enter starts a new line.
Ctrl-C or Esc stops what is running. A prompt sent while it runs waits its turn.
Ctrl-T shows or hides the model's thinking. Ctrl-L clears the screen. Ctrl-D leaves.

/diff [--names]        what this session would change in your tree
/apply [--all] [PATH]  take its changes; what runs later waits for its name, or --all
/undo                  take back what apply did
/use [local] [OPTIONS] which model; switch to it and make sure it is loaded
/llama [start|stop]    the local server by hand; stopping it frees what it held
/sessions              every session, and what each holds
/new                   a new session; this one stays as it is
/quit                  leave
A prompt that starts with // sends the rest with one /.";

struct Tty {
    saved: sys::Termios,
    stderr: i32,
}

/// What to put back, for whatever ends this: a signal handler and a panic can only reach a static.
static TTY: OnceLock<Tty> = OnceLock::new();

fn restore() {
    if let Some(t) = TTY.get() {
        unsafe {
            sys::tcsetattr(0, sys::TCSANOW, &t.saved);
            sys::write(1, screen::LEAVE.as_ptr().cast(), screen::LEAVE.len());
            sys::dup2(t.stderr, 2);
        }
    }
}

extern "C" fn on_signal(signal: i32) {
    restore();
    unsafe { sys::_exit(128 + signal) }
}

/// Raw mode, and stderr to a file: a stray line from anything would land in the live rows.
fn enter(dir: &Path) -> Result<(), String> {
    let mut saved = sys::Termios::zeroed();
    if unsafe { sys::tcgetattr(0, &mut saved) } != 0 {
        return Err("this terminal cannot be set up".into());
    }
    let stderr = unsafe { sys::dup(2) };
    if stderr < 0 || unsafe { sys::fcntl(stderr, sys::F_SETFD, sys::FD_CLOEXEC) } != 0 {
        return Err("stderr cannot be kept".into());
    }
    let log = fs::File::create(dir.join("stderr")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _ = TTY.set(Tty { saved, stderr });
    let mut raw = saved;
    unsafe {
        sys::dup2(log.as_raw_fd(), 2);
        sys::cfmakeraw(&mut raw);
        if sys::tcsetattr(0, sys::TCSANOW, &raw) != 0 {
            restore();
            return Err("this terminal cannot be set to raw".into());
        }
        sys::signal(sys::SIGTERM, on_signal as *const () as usize);
        sys::signal(sys::SIGHUP, on_signal as *const () as usize);
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        hook(info);
        std::process::exit(101);
    }));
    let mut out = std::io::stdout();
    out.write_all(screen::ENTER.as_bytes()).and_then(|_| out.flush()).map_err(|e| e.to_string())
}

fn size() -> (usize, usize) {
    let mut w = sys::Winsize::default();
    let got = unsafe { sys::ioctl(1, sys::TIOCGWINSZ, &mut w as *mut sys::Winsize) };
    if got != 0 || w.cols == 0 || w.rows == 0 {
        return (80, 24);
    }
    (w.cols as usize, w.rows as usize)
}

enum Event {
    Key(Key),
    /// The terminal has gone.
    Gone,
    /// A model's words, cleaned.
    Words(String),
    /// Its thinking, cleaned.
    Thinking(String),
    Line(Style, String),
    Phase(String),
    Usage(u64, Option<f64>),
    Context(u64),
    /// What the status row says when nothing is running.
    State { id: String, held: usize, model: String, said: usize },
    Done,
    Closed,
}

enum Job {
    Hello,
    Prompt(String),
    Diff(bool),
    Apply(bool, Vec<PathBuf>),
    Undo,
    Use(Vec<OsString>),
    Llama(Vec<OsString>),
    Sessions,
    New,
}

/// What is shown of a call's answer: nothing for what worked, and for a command that failed, its
/// code and its last lines, which say most about why.
fn detail(name: &str, ok: bool, answer: &str) -> (bool, Vec<String>) {
    let failed = !ok || (name == "shell" && !answer.starts_with("exit 0"));
    if !failed {
        return (false, Vec::new());
    }
    let lines: Vec<&str> = answer.lines().collect();
    let mut shown: Vec<String> = lines.first().map(|l| l.to_string()).into_iter().collect();
    if name == "shell" {
        let body = &lines[1.min(lines.len())..];
        shown.extend(body[body.len().saturating_sub(FAILED_LINES_SHOWN)..].iter().map(|l| l.to_string()));
    }
    (true, shown.into_iter().map(|l| l.chars().take(200).collect()).collect())
}

struct Reporter {
    tx: Sender<Event>,
    transcript: fs::File,
    clean: term::Clean,
}

impl Report for Reporter {
    fn tool(&mut self, name: &str, args: &Value, ok: bool, answer: &str) {
        let (failed, lines) = detail(name, ok, answer);
        let (style, mark) = if failed { (Style::Bad, '✗') } else { (Style::Good, '✓') };
        let _ = self.tx.send(Event::Line(style, format!("  {mark} {}", agent::one_line(&term::scrub(name), args))));
        for l in lines {
            let _ = self.tx.send(Event::Line(Style::Dim, format!("    {l}")));
        }
    }

    fn note(&mut self, text: &str) {
        let _ = self.tx.send(Event::Line(Style::Note, format!("  {text}")));
    }

    fn usage(&mut self, u: &Usage) {
        let _ = self.tx.send(Event::Usage(u.prompt_tokens + u.completion_tokens, u.tokens_per_second));
    }

    fn message(&mut self, m: &Value) {
        let mut line = m.text();
        line.push('\n');
        if let Err(e) = self.transcript.write_all(line.as_bytes()) {
            let _ = self.tx.send(Event::Line(Style::Bad, format!("  the transcript could not be written: {e}")));
        }
    }

    fn delta(&mut self, d: Delta) {
        // Cleaned here, so what is measured for wrapping is what will be shown.
        let e = match d {
            Delta::Content(t) => Event::Words(self.clean.push(t.as_bytes())),
            Delta::Reasoning(t) => Event::Thinking(self.clean.push(t.as_bytes())),
        };
        let _ = self.tx.send(e);
    }

    fn phase(&mut self, what: &str) {
        let _ = self.tx.send(Event::Phase(what.to_string()));
    }

    fn context(&mut self, tokens: u64) {
        let _ = self.tx.send(Event::Context(tokens));
    }
}

/// What the gate and the commands write, a line at a time.
struct Lines<'a> {
    tx: &'a Sender<Event>,
    style: Style,
    diff: bool,
    buf: Vec<u8>,
}

/// A diff's colours. Each line of a file starts with a mark the gate chose, so no file can look
/// like an added line it is not.
fn diff_style(line: &str) -> Style {
    if line.starts_with("--- ") || line.starts_with("@@") {
        Style::Hunk
    } else if line.starts_with('+') {
        Style::Added
    } else if line.starts_with('-') {
        Style::Removed
    } else {
        Style::Plain
    }
}

impl Lines<'_> {
    fn send(&self, line: &str) {
        let style = if self.diff { diff_style(line) } else { self.style };
        let _ = self.tx.send(Event::Line(style, line.to_string()));
    }

    fn finish(mut self) {
        if !self.buf.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned();
            self.send(&rest);
        }
    }
}

impl Write for Lines<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        while let Some(i) = self.buf.iter().position(|c| *c == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            self.send(String::from_utf8_lossy(&line[..i]).as_ref());
        }
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Said when it changes: the status row has it the rest of the time.
fn say_held(c: &Conversation, before: usize, tx: &Sender<Event>) {
    let held = c.held().unwrap_or_default();
    if held.len() == before || held.is_empty() {
        return;
    }
    let waiting = held.iter().filter(|h| !h.flags.is_empty()).count();
    let note = if waiting > 0 { format!(", {waiting} flagged to wait for your word") } else { String::new() };
    let _ = tx.send(Event::Line(Style::Note, format!("  {} held{note}; your tree is unchanged. /diff shows them, /apply takes them.", changes(held.len()))));
}

fn changes(n: usize) -> String {
    if n == 1 { "1 change".to_string() } else { format!("{n} changes") }
}

fn do_job(c: &mut Conversation, job: Job, tx: &Sender<Event>, cancel: &Cancel) -> Result<(), String> {
    let home = c.home().to_path_buf();
    let quiet = |style| Lines { tx, style, diff: false, buf: Vec::new() };
    let mut say = |t: &str| {
        let _ = tx.send(Event::Line(Style::Note, format!("  {t}")));
    };
    match job {
        Job::Hello => {}
        Job::Prompt(text) => {
            let mut r = Reporter { tx: tx.clone(), transcript: c.transcript()?, clean: term::Clean::default() };
            let before = c.held().map_or(0, |h| h.len());
            let end = c.prompt(&text, &mut r, cancel)?;
            match end {
                End::Done => {}
                End::Stopped(why) => say(&format!("stopped: {why}")),
                End::Interrupted => say("interrupted"),
            }
            say_held(c, before, tx);
        }
        Job::Diff(names) => {
            let mut w = Lines { tx, style: Style::Plain, diff: !names, buf: Vec::new() };
            c.diff(names, &mut w)?;
            w.finish();
        }
        Job::Apply(all, named) => {
            let mut w = quiet(Style::Good);
            c.apply(all, &named, &mut w)?;
            w.finish();
        }
        Job::Undo => {
            let mut w = quiet(Style::Good);
            c.undo(&mut w)?;
            w.finish();
        }
        Job::Use(args) => {
            let line = serve::use_local(&home, &args, &mut say, cancel)?;
            let _ = tx.send(Event::Line(Style::Dim, line));
        }
        Job::Llama(args) => {
            let line = serve::llama(&home, &args, &mut say, cancel)?;
            let _ = tx.send(Event::Line(Style::Dim, line));
        }
        Job::Sessions => {
            let mut w = quiet(Style::Dim);
            gate::sessions(&home, &mut w)?;
            w.finish();
        }
        Job::New => {
            let id = format!("s{}", agent::now());
            if id == c.id {
                return Err("a session began this second: try again".into());
            }
            let fresh = Conversation::open(Some(id), Some(c.project.clone()), c.max_turns(), c.limit())?;
            let old = std::mem::replace(c, fresh);
            let (old_id, held) = (old.id.clone(), old.held().map_or(0, |h| h.len()));
            old.close()?;
            if held > 0 {
                say(&format!("session {old_id} keeps what it holds: {}. `writ apply {old_id}` takes it", changes(held)));
            }
            let _ = tx.send(Event::Line(Style::Dim, format!("session {}", c.id)));
        }
    }
    Ok(())
}

/// The work: one job at a time, until the screen lets go of the other end.
fn worker(mut c: Conversation, jobs: Receiver<Job>, tx: Sender<Event>, cancel: Cancel) {
    for job in jobs {
        let hello = matches!(job, Job::Hello);
        if let Err(e) = do_job(&mut c, job, &tx, &cancel) {
            let _ = tx.send(Event::Line(Style::Bad, format!("  {e}")));
        }
        let model = serve::status(c.home());
        let held = c.held().map_or(0, |h| h.len());
        let _ = tx.send(Event::State { id: c.id.clone(), held, model, said: c.messages() });
        if !hello {
            let _ = tx.send(Event::Done);
        }
    }
    if let Err(e) = c.close() {
        eprintln!("writ: {e}");
    }
    let _ = tx.send(Event::Closed);
}

fn read_keys(tx: Sender<Event>) {
    let mut decoder = Decoder::default();
    let mut buf = [0u8; 4096];
    loop {
        let wait = if decoder.waiting_on_escape() { ESCAPE_WAIT_MS } else { -1 };
        let mut keys = Vec::new();
        if sys::wait_readable(0, wait) {
            let n = unsafe { sys::read(0, buf.as_mut_ptr().cast(), buf.len()) };
            if n == 0 || (n < 0 && sys::errno() != sys::EINTR) {
                let _ = tx.send(Event::Gone);
                return;
            }
            if n > 0 {
                decoder.push(&buf[..n as usize], &mut keys);
            }
        } else {
            decoder.flush_escape(&mut keys);
        }
        for k in keys {
            if tx.send(Event::Key(k)).is_err() {
                return;
            }
        }
    }
}

/// A model's words on their way up. Finished rows go as soon as they are full, so a long paragraph
/// flows; code is never broken at a space, since it would then not paste back.
#[derive(Default)]
struct Stream {
    partial: String,
    code: bool,
    style: Option<Style>,
}

fn fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

impl Stream {
    fn line_style(&self) -> Style {
        match self.style {
            Some(Style::Dim) => Style::Dim,
            _ if self.code => Style::Code,
            _ => Style::Plain,
        }
    }

    fn push(&mut self, text: &str, style: Style, cols: usize, up: &mut Vec<Row>) {
        if self.style != Some(style) {
            self.flush(cols, up);
            self.style = Some(style);
        }
        for piece in text.replace('\t', "    ").split_inclusive('\n') {
            match piece.strip_suffix('\n') {
                Some(body) => {
                    self.partial.push_str(body);
                    self.end_line(cols, up);
                }
                None => {
                    self.partial.push_str(piece);
                    if !self.code && !fence(&self.partial) {
                        let mut rows = screen::wrap(&self.partial, cols);
                        self.partial = rows.pop().unwrap_or_default();
                        let s = self.line_style();
                        up.extend(rows.into_iter().map(|r| Row::new(s, r)));
                    }
                }
            }
        }
    }

    fn end_line(&mut self, cols: usize, up: &mut Vec<Row>) {
        let line = std::mem::take(&mut self.partial);
        if fence(&line) {
            self.code = !self.code;
            up.push(Row::new(if self.style == Some(Style::Dim) { Style::Dim } else { Style::Code }, line));
        } else if self.code {
            up.push(Row::new(self.line_style(), line));
        } else {
            let s = self.line_style();
            up.extend(screen::wrap(&line, cols).into_iter().map(|r| Row::new(s, r)));
        }
    }

    /// What is left goes up, and an unclosed fence does not outlive what opened it.
    fn flush(&mut self, cols: usize, up: &mut Vec<Row>) {
        if !self.partial.is_empty() {
            self.end_line(cols, up);
        }
        self.code = false;
    }

    fn live(&self) -> Option<Row> {
        (!self.partial.is_empty()).then(|| Row::new(self.line_style(), self.partial.clone()))
    }
}

#[derive(Debug, PartialEq)]
enum Typed {
    Prompt(String),
    Command(String, Vec<String>),
}

/// A command is one line, starting with a slash and a plain word, so `/etc/passwd is wrong` is a prompt.
fn read_typed(text: &str) -> Typed {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("//") {
        return Typed::Prompt(format!("/{rest}"));
    }
    if let Some(rest) = t.strip_prefix('/') {
        let mut words = rest.split_whitespace();
        if let Some(first) = words.next() {
            if !t.contains('\n') && first.chars().all(|c| c.is_ascii_lowercase()) {
                return Typed::Command(first.to_string(), words.map(str::to_string).collect());
            }
        }
    }
    Typed::Prompt(text.to_string())
}

fn os(words: Vec<String>) -> Vec<OsString> {
    words.into_iter().map(OsString::from).collect()
}

struct Ui {
    screen: Screen,
    editor: Editor,
    stream: Stream,
    /// Finished rows, waiting for the next frame.
    up: Vec<Row>,
    jobs: Sender<Job>,
    cancel: Cancel,
    running: Option<Instant>,
    phase: String,
    tokens: u64,
    speed: Option<f64>,
    used: u64,
    context: u64,
    queued: VecDeque<String>,
    id: String,
    held: usize,
    model: String,
    said: usize,
    project: String,
    show_thinking: bool,
    quit_armed: bool,
    quitting: bool,
    in_prose: bool,
    last_blank: bool,
    dirty: bool,
    closed: bool,
}

impl Ui {
    fn say(&mut self, style: Style, text: impl Into<String>) {
        let cols = self.screen.cols();
        self.stream.flush(cols, &mut self.up);
        self.in_prose = false;
        self.last_blank = false;
        self.up.push(Row::new(style, text));
        self.dirty = true;
    }

    /// A word to the person, above what the model is halfway through saying, which it leaves alone.
    fn aside(&mut self, style: Style, text: impl Into<String>) {
        self.up.push(Row::new(style, text));
        self.dirty = true;
    }

    fn blank(&mut self) {
        let cols = self.screen.cols();
        self.stream.flush(cols, &mut self.up);
        if !self.last_blank {
            self.up.push(Row::new(Style::Plain, ""));
            self.last_blank = true;
        }
    }

    fn begin(&mut self, job: Job, phase: &str) {
        // Here and not in the worker: a stop right after this must not be lost.
        self.cancel.reset();
        self.running = Some(Instant::now());
        self.phase = phase.to_string();
        self.tokens = 0;
        self.speed = None;
        let _ = self.jobs.send(job);
    }

    fn start_prompt(&mut self, text: String) {
        self.blank();
        let cols = self.screen.cols().saturating_sub(2);
        let mut first = true;
        for line in text.lines() {
            for row in screen::wrap(line, cols) {
                self.say(Style::You, format!("{}{row}", if first { "> " } else { "  " }));
                first = false;
            }
        }
        self.begin(Job::Prompt(text), "starting");
    }

    fn submit(&mut self, text: String) {
        self.editor.remember(&text);
        match read_typed(&text) {
            Typed::Prompt(p) if self.running.is_some() => {
                self.queued.push_back(p);
                self.dirty = true;
            }
            Typed::Prompt(p) => self.start_prompt(p),
            Typed::Command(name, args) => self.command(&name, args, &text),
        }
    }

    fn command(&mut self, name: &str, args: Vec<String>, typed: &str) {
        match name {
            "help" => {
                self.blank();
                for l in HELP.lines() {
                    self.say(Style::Dim, l);
                }
                return;
            }
            "quit" | "exit" => {
                self.quit();
                return;
            }
            _ => {}
        }
        let job = match name {
            "diff" => Job::Diff(args.iter().any(|a| a == "--names")),
            "apply" => {
                let all = args.iter().any(|a| a == "--all");
                Job::Apply(all, args.into_iter().filter(|a| a != "--all").map(PathBuf::from).collect())
            }
            "undo" => Job::Undo,
            "use" | "model" => Job::Use(os(args)),
            "llama" => Job::Llama(os(args)),
            "sessions" => Job::Sessions,
            "new" => Job::New,
            _ => {
                self.editor.set(typed);
                self.aside(Style::Bad, format!("  there is no /{name}: /help lists them, and // sends it to the model"));
                return;
            }
        };
        if self.running.is_some() {
            self.editor.set(typed);
            self.aside(Style::Note, "  that waits until this stops: Ctrl-C stops it now");
            return;
        }
        self.blank();
        self.say(Style::You, format!("> {}", typed.trim()));
        self.begin(job, "working");
    }

    fn quit(&mut self) {
        self.cancel.fire();
        self.quitting = true;
    }

    fn key(&mut self, key: Key) {
        let armed = std::mem::take(&mut self.quit_armed);
        self.dirty = true;
        match key {
            Key::Interrupt | Key::Esc if self.running.is_some() => {
                self.cancel.fire();
                self.phase = "stopping".to_string();
            }
            Key::Esc => {}
            Key::Interrupt if !self.editor.is_empty() => self.editor.clear(),
            Key::Interrupt if armed => self.quit(),
            Key::Interrupt => {
                self.quit_armed = true;
                self.aside(Style::Dim, "  Ctrl-C again, or Ctrl-D, leaves");
            }
            Key::Eof if self.editor.is_empty() => {
                if self.running.is_none() {
                    self.quit();
                }
            }
            Key::Clear => {
                let bytes = self.screen.clear();
                self.write(&bytes);
            }
            Key::Thinking => {
                self.show_thinking = !self.show_thinking;
                let word = if self.show_thinking { "shown" } else { "hidden" };
                self.aside(Style::Dim, format!("  the model's thinking is {word}"));
            }
            other => {
                if let Edit::Submit(text) = self.editor.key(&other) {
                    self.submit(text);
                }
            }
        }
    }

    fn event(&mut self, e: Event) {
        let cols = self.screen.cols();
        self.dirty = true;
        match e {
            Event::Key(k) => self.key(k),
            Event::Gone => self.quit(),
            Event::Words(t) => {
                self.tokens += 1;
                if !self.in_prose {
                    self.blank();
                    self.in_prose = true;
                }
                self.last_blank = false;
                self.stream.push(&t, Style::Plain, cols, &mut self.up);
            }
            Event::Thinking(t) => {
                self.tokens += 1;
                if self.show_thinking {
                    self.last_blank = false;
                    self.stream.push(&t, Style::Dim, cols, &mut self.up);
                }
            }
            Event::Line(style, text) => self.say(style, text),
            Event::Phase(p) => self.phase = p,
            Event::Usage(used, speed) => {
                self.used = used;
                self.speed = speed;
            }
            Event::Context(n) => self.context = n,
            Event::State { id, held, model, said } => {
                self.id = id;
                self.held = held;
                self.model = model;
                self.said = said;
            }
            Event::Done => {
                self.running = None;
                self.stream.flush(cols, &mut self.up);
                if let Some(next) = self.queued.pop_front().filter(|_| !self.quitting) {
                    self.start_prompt(next);
                }
            }
            Event::Closed => self.closed = true,
        }
    }

    fn status(&self) -> Row {
        let mut s = String::new();
        match self.running {
            Some(since) => {
                let spin = SPINNER[(since.elapsed().as_millis() / TICK.as_millis()) as usize % SPINNER.len()];
                s.push_str(&format!("{spin} {} · {}s", self.phase, since.elapsed().as_secs()));
                if self.tokens > 0 {
                    s.push_str(&format!(" · {} tokens", self.tokens));
                }
                if let Some(v) = self.speed {
                    s.push_str(&format!(" · {v:.0}/s"));
                }
            }
            None => s.push_str(&format!("{} · {} · {} held", self.id, self.project, self.held)),
        }
        if self.context > 0 && self.used > 0 {
            s.push_str(&format!(" · context {}%", self.used * 100 / self.context));
        }
        // Last, as the longest: a narrow window loses this first.
        if self.running.is_some() {
            s.push_str(" · Ctrl-C stops");
        } else {
            s.push_str(&format!(" · {}", self.model));
        }
        Row::new(Style::Dim, s)
    }

    fn live(&self) -> (Vec<Row>, (usize, usize)) {
        let cols = self.screen.cols();
        let mut rows: Vec<Row> = self.stream.live().into_iter().collect();
        for q in self.queued.iter().take(QUEUED_SHOWN) {
            rows.push(Row::new(Style::Dim, format!("queued: {}", q.lines().next().unwrap_or(""))));
        }
        rows.push(self.status());
        let (input, cursor) = self.editor.layout(cols);
        // A long prompt shows the part with the cursor in it, and the status stays in sight.
        let room = self.screen.rows().saturating_sub(rows.len() + 1).max(1);
        let start = cursor.0.saturating_sub(room - 1);
        let at = (rows.len() + cursor.0 - start, cursor.1);
        rows.extend(input.into_iter().skip(start).take(room).map(|r| Row::new(Style::Plain, r)));
        (rows, at)
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut out = std::io::stdout();
        if out.write_all(bytes).and_then(|_| out.flush()).is_err() {
            self.quit();
        }
    }

    fn draw(&mut self) {
        let (live, cursor) = self.live();
        let up = std::mem::take(&mut self.up);
        let bytes = self.screen.frame(&up, &live, cursor);
        self.write(&bytes);
        self.dirty = false;
    }

    fn run(&mut self, rx: &Receiver<Event>) {
        let mut last_draw = Instant::now() - FRAME;
        let mut last_tick = Instant::now();
        while !self.quitting {
            let wait = if self.dirty { FRAME } else if self.running.is_some() { TICK } else { IDLE_TICK };
            let mut keyed = false;
            match rx.recv_timeout(wait) {
                Ok(e) => {
                    keyed |= matches!(e, Event::Key(_));
                    self.event(e);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(e) = rx.try_recv() {
                keyed |= matches!(e, Event::Key(_));
                self.event(e);
                if self.quitting {
                    break;
                }
            }
            let (cols, rows) = size();
            if (cols, rows) != (self.screen.cols(), self.screen.rows()) {
                self.screen.resize(cols, rows);
                self.dirty = true;
            }
            if self.running.is_some() && last_tick.elapsed() >= TICK {
                last_tick = Instant::now();
                self.dirty = true;
            }
            if self.dirty && (keyed || last_draw.elapsed() >= FRAME) {
                self.draw();
                last_draw = Instant::now();
            }
        }
    }
}

/// `writ`, on a terminal.
pub fn main(args: &[OsString]) -> Result<i32, String> {
    let mut project: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut max_turns: u32 = 100;
    let mut limit = Duration::from_secs(1800);
    let mut it = args.iter();
    let number = |v: Option<&OsString>, what: &str| -> Result<u64, String> {
        v.and_then(|s| s.to_str()).and_then(|s| s.parse().ok()).ok_or(format!("{what} takes a number"))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--project") => project = it.next().map(PathBuf::from),
            Some("--session") => name = it.next().map(|s| s.to_string_lossy().into_owned()),
            Some("--max-turns") => max_turns = number(it.next(), "--max-turns")? as u32,
            Some("--timeout") => limit = Duration::from_secs(number(it.next(), "--timeout")?),
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    if unsafe { sys::isatty(0) != 1 || sys::isatty(1) != 1 } {
        return Err("this needs a terminal. `writ do PROMPT` is for everything else".into());
    }
    let conversation = Conversation::open(name, project, max_turns, limit)?;
    let (id, project, prompts, carrying_on) = (conversation.id.clone(), conversation.project.clone(), conversation.prompts(), !conversation.is_new());
    enter(conversation.dir())?;

    let (tx, rx) = mpsc::channel();
    let (jobs, job_rx) = mpsc::channel();
    let cancel = Cancel::default();
    {
        let tx = tx.clone();
        std::thread::spawn(move || read_keys(tx));
    }
    {
        let (tx, cancel) = (tx.clone(), cancel.clone());
        std::thread::spawn(move || worker(conversation, job_rx, tx, cancel));
    }
    drop(tx);

    let (cols, rows) = size();
    let mut editor = Editor::default();
    for p in &prompts {
        editor.remember(p);
    }
    let name_of = project.file_name().map_or_else(|| project.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut ui = Ui {
        screen: Screen::new(cols, rows),
        editor,
        stream: Stream::default(),
        up: Vec::new(),
        jobs,
        cancel,
        running: None,
        phase: String::new(),
        tokens: 0,
        speed: None,
        used: 0,
        context: 0,
        queued: VecDeque::new(),
        id: id.clone(),
        held: 0,
        model: "looking".to_string(),
        said: prompts.len(),
        project: name_of,
        show_thinking: false,
        quit_armed: false,
        quitting: false,
        in_prose: false,
        last_blank: false,
        dirty: true,
        closed: false,
    };
    ui.say(Style::Dim, format!("writ: session {id} on {}", project.display()));
    if carrying_on {
        ui.say(Style::Dim, format!("carrying on, after {} prompts. Up brings them back.", prompts.len()));
    }
    ui.say(Style::Dim, "The model works in a box, and nothing reaches your tree until you /apply it. /help says the rest.");
    if landlock::abi() == 0 {
        ui.say(Style::Note, "this kernel has no Landlock; the box rests on namespaces and seccomp");
    }
    let _ = ui.jobs.send(Job::Hello);
    ui.run(&rx);

    // The worker ends what it is doing, and tidies a session nothing happened in.
    let (dummy, _) = mpsc::channel();
    drop(std::mem::replace(&mut ui.jobs, dummy));
    let start = Instant::now();
    while !ui.closed && start.elapsed() < Duration::from_secs(3) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Closed) => ui.closed = true,
            Ok(Event::State { id, held, model, said }) => {
                (ui.id, ui.held, ui.model, ui.said) = (id, held, model, said);
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let bytes = ui.screen.leave();
    let mut out = std::io::stdout();
    let _ = out.write_all(&bytes);
    let _ = out.flush();
    restore();

    let id = &ui.id;
    if ui.said > 0 || ui.held > 0 {
        let mut line = format!("writ: session {id}");
        if ui.held > 0 {
            line.push_str(&format!(": {} held; your tree is unchanged. `writ diff {id}`, then `writ apply {id}`", changes(ui.held)));
        }
        println!("{line}. `writ --session {id}` goes on.");
    }
    if ui.model.starts_with("load") {
        println!("writ: the model stays loaded, holding its memory: `writ llama stop` frees it.");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Typed {
        read_typed(s)
    }

    #[test]
    fn a_command_is_a_slash_and_a_plain_word() {
        assert_eq!(typed("/diff"), Typed::Command("diff".into(), vec![]));
        assert_eq!(typed(" /apply --all src/a.rs "), Typed::Command("apply".into(), vec!["--all".into(), "src/a.rs".into()]));
        assert_eq!(typed("fix /diff please"), Typed::Prompt("fix /diff please".into()));
    }

    #[test]
    fn a_path_or_a_second_line_is_a_prompt() {
        assert_eq!(typed("/etc/passwd is wrong"), Typed::Prompt("/etc/passwd is wrong".into()));
        assert_eq!(typed("/apply\nand then something"), Typed::Prompt("/apply\nand then something".into()));
        assert_eq!(typed("//apply this"), Typed::Prompt("/apply this".into()));
        assert_eq!(typed("/"), Typed::Prompt("/".into()));
    }

    fn pushed(chunks: &[&str], cols: usize) -> (Vec<Row>, Stream) {
        let mut s = Stream::default();
        let mut up = Vec::new();
        for c in chunks {
            s.push(c, Style::Plain, cols, &mut up);
        }
        (up, s)
    }

    fn texts(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn rows_go_up_as_they_fill_however_the_text_arrives() {
        let (up, s) = pushed(&["the quick ", "brown", " fox jumps ", "over"], 12);
        assert_eq!(texts(&up), ["the quick", "brown fox"]);
        assert_eq!(s.partial, "jumps over");
    }

    #[test]
    fn a_line_ending_sends_it_and_blank_lines_survive() {
        let (up, s) = pushed(&["one\n\ntwo\n"], 40);
        assert_eq!(texts(&up), ["one", "", "two"]);
        assert!(s.live().is_none());
    }

    #[test]
    fn code_is_never_broken_at_a_space() {
        let long = "let x = a_long_name + another_long_name + yet_another_long_name;";
        let (up, s) = pushed(&["before\n```rust\n", long, "\n```\nafter"], 20);
        assert_eq!(texts(&up), ["before", "```rust", long, "```"]);
        assert_eq!(up[2].style, Style::Code);
        assert_eq!(s.partial, "after");
        assert_eq!(s.live().unwrap().style, Style::Plain);
    }

    #[test]
    fn a_models_words_are_plain_and_thinking_is_dim() {
        let mut s = Stream::default();
        let mut up = Vec::new();
        s.push("hmm\n", Style::Dim, 40, &mut up);
        s.push("so", Style::Plain, 40, &mut up);
        s.push(" it is\n", Style::Plain, 40, &mut up);
        assert_eq!(up, [Row::new(Style::Dim, "hmm"), Row::new(Style::Plain, "so it is")]);
    }

    #[test]
    fn an_unclosed_fence_ends_with_its_turn() {
        let (mut up, mut s) = pushed(&["```\nleft open"], 40);
        s.flush(40, &mut up);
        assert_eq!(texts(&up), ["```", "left open"]);
        assert!(!s.code);
    }

    #[test]
    fn what_shows_of_an_answer() {
        assert_eq!(detail("read", true, "the whole file\nand more"), (false, vec![]));
        assert_eq!(detail("shell", true, "exit 0\nall fine"), (false, vec![]));
        let (failed, lines) = detail("shell", true, "exit 101\na\nb\nc\nd\ne");
        assert!(failed);
        assert_eq!(lines, ["exit 101", "c", "d", "e"]);
        assert_eq!(detail("edit", false, "not found\nsecond"), (true, vec!["not found".to_string()]));
    }

    #[test]
    fn a_diff_is_coloured_by_the_gates_marks() {
        assert_eq!(diff_style("+added"), Style::Added);
        assert_eq!(diff_style("-removed"), Style::Removed);
        assert_eq!(diff_style("--- src/a.rs"), Style::Hunk);
        assert_eq!(diff_style("@@ -1 +1 @@"), Style::Hunk);
        // A line of a file starts with one of the gate's marks, so this is context, not an addition.
        assert_eq!(diff_style(" +looks added"), Style::Plain);
        assert_eq!(diff_style("M src/a.rs"), Style::Plain);
    }

    #[test]
    fn lines_are_cut_at_their_ends_and_the_last_is_not_lost() {
        let (tx, rx) = mpsc::channel();
        let mut w = Lines { tx: &tx, style: Style::Good, diff: false, buf: Vec::new() };
        w.write_all(b"one\ntw").unwrap();
        w.write_all(b"o\nthree").unwrap();
        w.finish();
        let got: Vec<String> = rx.try_iter().filter_map(|e| if let Event::Line(_, t) = e { Some(t) } else { None }).collect();
        assert_eq!(got, ["one", "two", "three"]);
    }
}
