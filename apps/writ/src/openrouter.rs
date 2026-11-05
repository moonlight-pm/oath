//! OpenRouter: a model on someone else's machine (R64). Everything else about a session stays as it
//! is on a local model: the tools run in their box, and the box has no route here. Three things are
//! new, and they live in this file. There is a key, which only this process and curl ever hold. What
//! the model reads leaves the machine, so every request asks for routes that keep nothing. And every
//! turn costs money, so a session has a budget, and what fails is tried again only when it can help.
//!
//! curl runs in a box of its own, with the network and nothing of `home`'s. It holds the key while it
//! talks, so an exploit of its TLS or HTTP has that and no more: no files, no `sudo`, and through
//! Landlock no port but HTTPS and DNS, so not the machine's SSH or anything else on the network.

use crate::chat::{self, Delta, Finish, Tool, Turn};
use crate::json::{self, n, obj, s, Value};
use crate::model::{self, Cancel, Endpoint, Reply};
use crate::{agent, inner, landlock, sandbox, serve, sys};
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const O_NOFOLLOW: i32 = 0o400000;
const KEY_NAME: &str = "openrouter.key";
const CHOICE_NAME: &str = "use.json";
const SPENT_NAME: &str = "spent";
const MAX_KEY: u64 = 1024;
/// What a session may spend when nobody has said. A cheap model does a lot for this, and a dear one
/// stops soon enough to be noticed. The key's own limit is what stops everything.
const DEFAULT_BUDGET: f64 = 1.0;
/// Of one turn. A reasoning model thinks first, and is billed by what it uses, not by this.
const MAX_OUTPUT: u64 = 32768;
const TRIES: u32 = 4;
/// Before each try again, when the server names no time of its own.
const BACKOFF_SECS: [u64; 3] = [2, 5, 15];
/// A wait past this is a limit to tell the person about, not one to sit through.
const MAX_WAIT_SECS: u64 = 300;

/// The box has what curl needs to run, to resolve a name and to check a certificate, whose roots are
/// in the store beside it. Not the rest of `/etc`.
const SYSTEM: &[&str] = &["/bin", "/lib", "/oath/store"];
const NAMES: &[&str] = &["/etc/resolv.conf", "/etc/hosts", "/etc/nsswitch.conf"];
const TMPFS_BYTES: u64 = 16 << 20;
/// HTTPS, and DNS for an answer too long for UDP.
const PORTS: &[u16] = &[443, 53];

pub struct Settings {
    pub model: String,
    pub context: u64,
    pub max_output: u64,
    pub budget: f64,
}

pub enum Choice {
    Local,
    OpenRouter(Settings),
}

fn config_dir(home: &Path) -> PathBuf {
    home.join(".config/writ")
}

fn choice_path(home: &Path) -> PathBuf {
    config_dir(home).join(CHOICE_NAME)
}

fn key_path(home: &Path) -> PathBuf {
    config_dir(home).join(KEY_NAME)
}

fn parse_choice(text: &str) -> Result<Choice, String> {
    let v = json::parse(text)?;
    match v.get("provider").and_then(|p| p.str()) {
        Some("local") => Ok(Choice::Local),
        Some("openrouter") => {
            let number = |k: &str| v.get(k).and_then(|x| x.u64()).ok_or(format!("no {k}"));
            Ok(Choice::OpenRouter(Settings {
                model: v.get("model").and_then(|m| m.str()).ok_or("no model")?.to_string(),
                context: number("context")?,
                max_output: number("max_output")?,
                budget: v.get("budget").and_then(|b| b.f64()).filter(|b| *b > 0.0).ok_or("no budget")?,
            }))
        }
        other => Err(format!("no provider called {other:?}")),
    }
}

fn choice_text(c: &Choice) -> String {
    match c {
        Choice::Local => obj([("provider", s("local"))]).text(),
        Choice::OpenRouter(o) => obj([
            ("provider", s("openrouter")),
            ("model", s(&o.model)),
            ("context", n(o.context)),
            ("max_output", n(o.max_output)),
            ("budget", Value::Num(o.budget.to_string())),
        ])
        .text(),
    }
}

/// What `writ use` last settled on. A local model, until it has settled on anything.
pub fn chosen(home: &Path) -> Result<Choice, String> {
    let path = choice_path(home);
    match fs::read_to_string(&path) {
        Ok(t) => parse_choice(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Choice::Local),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn save_choice(home: &Path, c: &Choice) -> Result<(), String> {
    let dir = config_dir(home);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = choice_path(home);
    fs::write(&path, choice_text(c)).map_err(|e| format!("{}: {e}", path.display()))
}

/// What can go in a header and a line of curl's config, whatever a file holds.
fn check_key(key: &str) -> Result<(), String> {
    let fit = !key.is_empty() && key.len() <= MAX_KEY as usize && key.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\\'));
    if !fit {
        return Err("that does not look like a key: it is one line, of letters, digits and punctuation".into());
    }
    // `OPENROUTER_API_KEY=sk-or-v1-...`, pasted from a file. Sent as it stands, OpenRouter refuses it
    // and says nothing of why. The line is a secret, so it is never in what comes back.
    if key.contains('=') {
        return Err("that is a line like NAME=value, not a key: it wants the key alone".into());
    }
    Ok(())
}

/// The key, from its file: read when it is needed and not kept, and refused if the file is a link, or
/// anyone but this account could have read it.
fn read_key(home: &Path) -> Result<String, String> {
    let path = key_path(home);
    let f = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "no OpenRouter key: `writ key` takes one".to_string(),
        _ => format!("{}: {e}", path.display()),
    })?;
    let m = f.metadata().map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if m.uid() != unsafe { sys::getuid() } {
        return Err(format!("{} is another account's", path.display()));
    }
    if m.mode() & 0o077 != 0 {
        return Err(format!("anyone could read {}: chmod 600 it, and make a new key", path.display()));
    }
    let mut text = String::new();
    f.take(MAX_KEY + 1).read_to_string(&mut text).map_err(|e| format!("{}: {e}", path.display()))?;
    let key = text.trim();
    check_key(key).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(key.to_string())
}

/// Whether there is a key that could be used, and not what it is.
pub fn check_key_file(home: &Path) -> Result<(), String> {
    read_key(home).map(|_| ())
}

fn save_key(home: &Path, key: &str) -> Result<(), String> {
    let dir = config_dir(home);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = key_path(home);
    let new = dir.join(format!("{KEY_NAME}.{}", std::process::id()));
    let _ = fs::remove_file(&new);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(O_NOFOLLOW).open(&new).map_err(|e| format!("{}: {e}", new.display()))?;
    f.write_all(key.as_bytes()).and_then(|_| f.write_all(b"\n")).and_then(|_| f.sync_all()).map_err(|e| format!("{}: {e}", new.display()))?;
    fs::rename(&new, &path).map_err(|e| format!("{}: {e}", path.display()))
}

static SAVED_TERMINAL: OnceLock<sys::Termios> = OnceLock::new();

fn put_echo_back() {
    if let Some(t) = SAVED_TERMINAL.get() {
        unsafe { sys::tcsetattr(0, sys::TCSANOW, t) };
    }
}

extern "C" fn on_signal(signal: i32) {
    put_echo_back();
    unsafe { sys::_exit(128 + signal) }
}

/// One line from a person, unseen, or from whatever is piped in.
fn read_secret() -> Result<String, String> {
    let terminal = unsafe { sys::isatty(0) } == 1;
    if terminal {
        let mut saved = sys::Termios::zeroed();
        if unsafe { sys::tcgetattr(0, &mut saved) } != 0 {
            return Err("this terminal cannot be set up".into());
        }
        let _ = SAVED_TERMINAL.set(saved);
        eprint!("OpenRouter key (it will not show): ");
        unsafe {
            sys::signal(sys::SIGINT, on_signal as *const () as usize);
            sys::signal(sys::SIGTERM, on_signal as *const () as usize);
            sys::signal(sys::SIGHUP, on_signal as *const () as usize);
            sys::tcsetattr(0, sys::TCSANOW, &saved.without_echo());
        }
    }
    let mut line = Vec::new();
    let got = std::io::stdin().lock().take(MAX_KEY + 1).read_until(b'\n', &mut line);
    if terminal {
        put_echo_back();
        eprintln!();
    }
    got.map_err(|e| e.to_string())?;
    String::from_utf8(line).map(|l| l.trim().to_string()).map_err(|_| "that does not look like a key".to_string())
}

enum KeyWord {
    Refused,
    Fine(String),
    NoAnswer(String),
}

/// What OpenRouter says of a key: whether it knows it, and what could be spent with it.
fn ask_about_key(key: &str) -> KeyWord {
    match model::get(&Endpoint::OpenRouter(Some(key.to_string())), "/v1/key") {
        Err(e) => KeyWord::NoAnswer(e),
        Ok((401 | 403, _)) => KeyWord::Refused,
        Ok((200, body)) => {
            let v = json::parse(&String::from_utf8_lossy(&body)).unwrap_or(Value::Null);
            let d = v.get("data");
            let number = |k: &str| d.and_then(|d| d.get(k)).and_then(|x| x.f64());
            KeyWord::Fine(match (number("limit"), number("limit_remaining")) {
                (Some(limit), Some(left)) => format!("this key has {} left of its {} limit", dollars(left), dollars(limit)),
                _ => "this key has no spend limit, so only writ's budget stands between a runaway and your balance: set one at openrouter.ai/settings/keys".to_string(),
            })
        }
        Ok((other, _)) => KeyWord::NoAnswer(format!("it answered {other}")),
    }
}

/// `writ key`.
pub fn key_main(args: &[OsString]) -> Result<i32, String> {
    if !args.is_empty() {
        return Err("writ key takes nothing: it asks, or reads what is piped in".into());
    }
    let home = sandbox::home()?;
    let key = read_secret()?;
    check_key(&key)?;
    let word = ask_about_key(&key);
    if matches!(word, KeyWord::Refused) {
        return Err("OpenRouter refused that key, and it was not saved".into());
    }
    save_key(&home, &key)?;
    println!("saved in {}, for this account only", key_path(&home).display());
    match word {
        KeyWord::Fine(t) => println!("{t}"),
        KeyWord::NoAnswer(e) => println!("OpenRouter could not be asked about it: {}", e.trim()),
        KeyWord::Refused => {}
    }
    Ok(0)
}

fn dollars(v: f64) -> String {
    if v < 0.01 && v != 0.0 {
        format!("${v:.4}")
    } else {
        format!("${v:.2}")
    }
}

fn price(v: f64) -> String {
    if v < 0.1 {
        format!("${v:.3}")
    } else {
        format!("${v:.2}")
    }
}

/// A model's name as OpenRouter writes it, `author/name` and maybe a `:variant`. It goes into a path.
fn good_model(m: &str) -> bool {
    let Some((author, name)) = m.split_once('/') else { return false };
    let plain = |t: &str| !t.is_empty() && !t.starts_with(['.', '-', ':']) && t.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'));
    plain(author) && plain(name) && !author.contains(':') && !m.contains("..")
}

/// What is known of a model's routes that keep no data and offer tools.
#[derive(Debug, PartialEq)]
struct Facts {
    routes: usize,
    caching: usize,
    context: u64,
    max_output: u64,
    prompt: (f64, f64),
    completion: (f64, f64),
}

/// From OpenRouter's list of routes that keep no data. The smallest context: any of them can be
/// chosen. The largest limit on output, or a limit past some routes would leave none.
fn facts_of(catalog: &Value, model: &str) -> Result<Facts, String> {
    let base = model.split(':').next().unwrap_or(model);
    let rows = catalog.get("data").and_then(|d| d.arr()).ok_or("OpenRouter's list of routes makes no sense")?;
    let of_model: Vec<&Value> = rows.iter().filter(|r| r.get("model_id").and_then(|m| m.str()) == Some(base)).collect();
    if of_model.is_empty() {
        return Err(format!("{model} has no route that promises to keep nothing"));
    }
    let tools = |r: &&Value| r.get("supported_parameters").and_then(|p| p.arr()).is_some_and(|p| p.iter().any(|x| x.str() == Some("tools")));
    let usable: Vec<&Value> = of_model.into_iter().filter(tools).collect();
    if usable.is_empty() {
        return Err(format!("no route for {model} that keeps nothing offers tools, and writ does not read calls out of text"));
    }
    let count = |k: &str| usable.iter().filter_map(|r| r.get(k).and_then(|x| x.u64())).collect::<Vec<u64>>();
    let rate = |k: &str| {
        let per_token = usable.iter().filter_map(|r| r.get("pricing")?.get(k)?.str()?.parse::<f64>().ok());
        let per_million: Vec<f64> = per_token.map(|p| p * 1e6).collect();
        let low = per_million.iter().copied().fold(f64::INFINITY, f64::min);
        let high = per_million.iter().copied().fold(0.0, f64::max);
        (if low.is_finite() { low } else { 0.0 }, high)
    };
    Ok(Facts {
        routes: usable.len(),
        caching: usable.iter().filter(|r| r.get("supports_implicit_caching") == Some(&Value::Bool(true))).count(),
        context: count("context_length").into_iter().min().ok_or(format!("no route for {model} says how long its context is"))?,
        max_output: count("max_completion_tokens").into_iter().max().unwrap_or(MAX_OUTPUT).min(MAX_OUTPUT),
        prompt: rate("prompt"),
        completion: rate("completion"),
    })
}

fn facts(model: &str) -> Result<Facts, String> {
    let open = Endpoint::OpenRouter(None);
    let (status, body) = model::get(&open, "/v1/endpoints/zdr")?;
    if status != 200 {
        return Err(format!("OpenRouter's list of routes answered {status}"));
    }
    let catalog = json::parse(&String::from_utf8_lossy(&body)).map_err(|e| format!("OpenRouter's list of routes: {e}"))?;
    let found = facts_of(&catalog, model);
    if found.is_err() {
        // Which is it: a model it has never heard of says more than one with no route.
        let base = model.split(':').next().unwrap_or(model);
        if let Ok((404, _)) = model::get(&open, &format!("/v1/models/{base}/endpoints")) {
            return Err(format!("OpenRouter has no model called {model}"));
        }
    }
    found
}

fn range((low, high): (f64, f64)) -> String {
    if price(low) == price(high) {
        price(low)
    } else {
        format!("{}–{}", price(low), price(high))
    }
}

fn say_settings(o: &Settings) -> String {
    format!("openrouter: {}, a {}-token context, at most {} a session", o.model, o.context, dollars(o.budget))
}

fn use_openrouter(home: &Path, args: &[OsString], say: &mut dyn FnMut(&str)) -> Result<String, String> {
    let before = match chosen(home) {
        Ok(Choice::OpenRouter(o)) => Some(o),
        _ => None,
    };
    let mut model = None;
    let mut budget = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--budget") => {
                let v: Option<f64> = it.next().and_then(|v| v.to_str()).and_then(|v| v.trim_start_matches('$').parse().ok());
                budget = Some(v.filter(|b| b.is_finite() && *b > 0.0).ok_or("--budget takes dollars, more than none")?);
            }
            Some(m) if !m.starts_with("--") && model.is_none() => model = Some(m.to_string()),
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    let model = model.or(before.as_ref().map(|o| o.model.clone())).ok_or("which model? `writ use openrouter AUTHOR/NAME`, as openrouter.ai/models writes it")?;
    if !good_model(&model) {
        return Err(format!("{model} is not a model's name: it is AUTHOR/NAME, as openrouter.ai/models writes it"));
    }
    say("asking OpenRouter about it");
    let f = facts(&model)?;
    let budget = budget.or(before.map(|o| o.budget)).unwrap_or(DEFAULT_BUDGET);
    let settings = Settings { model, context: f.context, max_output: f.max_output, budget };
    say(&format!(
        "what the model reads leaves this machine, for OpenRouter and the host it picks: {} routes for this model promise to keep nothing, which cannot be checked from here",
        f.routes
    ));
    say(&format!("{} a million tokens in, {} out; {} of the {} routes cache what they have read", range(f.prompt), range(f.completion), f.caching, f.routes));
    match read_key(home) {
        Err(e) => say(&e),
        Ok(key) => match ask_about_key(&key) {
            KeyWord::Fine(t) => say(&t),
            KeyWord::Refused => say("OpenRouter refuses the key it has: `writ key` takes a new one"),
            KeyWord::NoAnswer(e) => say(&format!("OpenRouter could not be asked about the key: {}", e.trim())),
        },
    }
    if serve::status(home).starts_with("load") {
        say("the local model stays loaded, holding its memory: `writ llama stop` frees it");
    }
    let c = Choice::OpenRouter(settings);
    save_choice(home, &c)?;
    let Choice::OpenRouter(o) = &c else { unreachable!() };
    Ok(say_settings(o))
}

/// Which model, or switch to one. What comes back is one line to show.
pub fn choose(home: &Path, args: &[OsString], say: &mut dyn FnMut(&str), cancel: &Cancel) -> Result<String, String> {
    match args.first().and_then(|a| a.to_str()) {
        None => Ok(match chosen(home)? {
            Choice::Local => format!("local: {}", serve::status(home)),
            Choice::OpenRouter(o) => {
                if let Err(e) = read_key(home) {
                    say(&e);
                }
                say_settings(&o)
            }
        }),
        Some("local") => {
            let line = serve::use_local(home, &args[1..], say, cancel)?;
            // Only now: a model that would not load is not what later sessions should try.
            save_choice(home, &Choice::Local)?;
            Ok(line)
        }
        Some("openrouter") => use_openrouter(home, &args[1..], say),
        Some(other) => Err(format!("{other} is not a model writ can use: `local`, or `openrouter AUTHOR/NAME`")),
    }
}

/// `writ use`.
pub fn use_main(args: &[OsString]) -> Result<i32, String> {
    println!("{}", choose(&sandbox::home()?, args, &mut |t| eprintln!("writ: {t}"), &Cancel::default())?);
    Ok(0)
}

/// One line on the model in use, for the status row.
pub fn status_line(home: &Path) -> String {
    match chosen(home) {
        Ok(Choice::OpenRouter(o)) => format!("{} on OpenRouter", o.model),
        Ok(Choice::Local) => serve::status(home),
        Err(e) => e,
    }
}

/// What a session has spent.
struct Spend {
    file: Option<PathBuf>,
    spent: f64,
    budget: f64,
    /// A turn did not say what it cost, so nothing here can be trusted to hold.
    unknown: bool,
}

impl Spend {
    fn add(&mut self, cost: f64) {
        self.spent += cost;
        if let Some(f) = &self.file {
            // If this fails, the session still counts; only one that goes on later starts lower.
            let _ = fs::write(f, self.spent.to_string());
        }
    }

    fn over(&self) -> Option<String> {
        if self.unknown {
            return Some("OpenRouter did not say what a turn cost, so the budget cannot hold: this stops here, and only the key's own limit is left".into());
        }
        (self.spent >= self.budget).then(|| format!("this session has spent {} of its {} budget: `/use openrouter --budget DOLLARS` gives it more", dollars(self.spent), dollars(self.budget)))
    }
}

/// What a session has spent so far.
pub fn spent(session: &Path) -> f64 {
    fs::read_to_string(session.join(SPENT_NAME)).ok().and_then(|t| t.trim().parse().ok()).filter(|v: &f64| v.is_finite() && *v >= 0.0).unwrap_or(0.0)
}

/// What happens after a reply.
#[derive(Debug, PartialEq)]
enum Next {
    Use,
    Wait(u64, String),
    Stop(String),
}

/// curl's own codes for a connection that failed or was lost: no name, no connection, a stream cut
/// short, timed out, a handshake, nothing or a broken send or receive.
fn lost_connection(exit: Option<i32>) -> bool {
    matches!(exit, Some(6 | 7 | 16 | 18 | 28 | 35 | 52 | 55 | 56 | 92))
}

/// Whether a failure can be helped by asking again, and when. From fields, not words: a status, the
/// stable `error_type`, and `limit_source`, which is a limit somebody set and no waiting ends, unless
/// it is the one that says when to come back. What is not known to pass does not get another try:
/// each one costs money, and a request that is wrong stays wrong.
fn decide(turn: &Turn, reply: &Reply, tries: u32) -> Next {
    let why = match &turn.finish {
        Finish::Error(e) => e.clone(),
        Finish::CutOff => "the stream was cut off".to_string(),
        _ => return Next::Use,
    };
    let meta = turn.error.as_ref().and_then(|e| e.get("metadata"));
    let word = |k: &str| meta.and_then(|m| m.get(k)).and_then(|x| x.str());
    let code = turn.error.as_ref().and_then(|e| e.get("code")).and_then(|c| c.u64()).map(|c| c as u16).or(reply.status.filter(|s| *s >= 400));
    let passes = match (word("limit_source"), word("error_type")) {
        (Some("openrouter_in_flight_budget"), _) => reply.retry_after.is_some(),
        (Some(_), _) => false,
        (_, Some("rate_limit_exceeded" | "provider_overloaded" | "provider_unavailable" | "server" | "timeout")) => true,
        (_, Some(_)) => false,
        _ => match code {
            Some(c) => matches!(c, 408 | 429 | 500 | 502 | 503 | 504 | 520..=524 | 529),
            None => turn.error.is_none() && (matches!(turn.finish, Finish::CutOff) || lost_connection(reply.exit)),
        },
    };
    if !passes || tries + 1 >= TRIES {
        return Next::Stop(match code {
            Some(401) => format!("{why}. OpenRouter refused the key: `writ key` takes a new one"),
            Some(404) if meta.is_none() => format!("{why}. writ asks only for routes that keep nothing and offer every parameter it sends"),
            _ => why,
        });
    }
    let secs = reply.retry_after.unwrap_or(BACKOFF_SECS[(tries as usize).min(BACKOFF_SECS.len() - 1)]);
    if secs > MAX_WAIT_SECS {
        return Next::Stop(format!("{why}. It says to come back in {secs} seconds"));
    }
    let what = code.map_or("the connection failed".to_string(), |c| format!("OpenRouter said {c}"));
    Next::Wait(secs, what)
}

/// A model on OpenRouter, for the loop.
pub struct Hosted {
    settings: Settings,
    home: PathBuf,
    /// Keeps a conversation on one host, so what it has read stays cached.
    session: Option<String>,
    spend: Spend,
    limit: Duration,
    cancel: Cancel,
    tools: &'static [Tool],
    record: Option<PathBuf>,
    #[cfg(test)]
    fake: Option<PathBuf>,
}

impl Hosted {
    /// For a session, whose spending is written down in its directory.
    pub fn for_session(settings: Settings, home: &Path, id: &str, dir: &Path, limit: Duration, cancel: Cancel, tools: &'static [Tool]) -> Hosted {
        let spend = Spend { file: Some(dir.join(SPENT_NAME)), spent: spent(dir), budget: settings.budget, unknown: false };
        Hosted { settings, home: home.to_path_buf(), session: Some(id.to_string()), spend, limit, cancel, tools, record: None, #[cfg(test)] fake: None }
    }

    /// For one question, outside any session.
    pub fn for_question(settings: Settings, home: &Path, limit: Duration, tools: &'static [Tool], record: Option<PathBuf>) -> Hosted {
        let spend = Spend { file: None, spent: 0.0, budget: f64::INFINITY, unknown: false };
        Hosted { settings, home: home.to_path_buf(), session: None, spend, limit, cancel: Cancel::default(), tools, record, #[cfg(test)] fake: None }
    }

    fn endpoint(&self) -> Result<Endpoint, String> {
        #[cfg(test)]
        if let Some(socket) = &self.fake {
            return Ok(Endpoint::Fake(socket.clone(), "test-key".to_string()));
        }
        Ok(Endpoint::OpenRouter(Some(read_key(&self.home)?)))
    }

    fn body(&self, messages: &[Value], max_tokens: u64) -> Result<String, String> {
        let wire = chat::for_model(messages, Some(&self.settings.model));
        let mut fields = chat::request_fields(&self.settings.model, &wire, self.tools, max_tokens)?;
        // OpenRouter always counts, and says this is a parameter it has no use for.
        fields.retain(|(k, _)| k != "stream_options");
        // On each request, as well as whatever is bound to the key. No route that keeps nothing means
        // no answer, and a route must really take what is asked, not silently drop a parameter.
        let provider = obj([("zdr", Value::Bool(true)), ("data_collection", s("deny")), ("require_parameters", Value::Bool(true))]);
        fields.push(("provider".to_string(), provider));
        if let Some(id) = &self.session {
            fields.push(("session_id".to_string(), s(id)));
        }
        Ok(Value::Obj(fields).text())
    }

    fn account(&mut self, turn: &Turn) {
        match turn.usage.as_ref().and_then(|u| u.cost) {
            Some(c) => self.spend.add(c),
            None if matches!(turn.finish, Finish::Stop | Finish::ToolCalls | Finish::Length) => self.spend.unknown = true,
            None => {}
        }
    }

    /// Until it is time, or the person stops it. Whether it is time.
    fn wait(&self, secs: u64) -> bool {
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            if self.cancel.fired() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        !self.cancel.fired()
    }
}

impl agent::Mind for Hosted {
    fn context(&self) -> u64 {
        self.settings.context
    }

    fn name(&self) -> Option<&str> {
        Some(&self.settings.model)
    }

    fn max_tokens(&self) -> u64 {
        self.settings.max_output
    }

    fn out_of_budget(&self) -> Option<String> {
        self.spend.over()
    }

    fn turn(&mut self, messages: &[Value], max_tokens: u64, on: &mut dyn FnMut(Delta)) -> Result<Turn, String> {
        let body = self.body(messages, max_tokens)?;
        let began = Instant::now();
        let mut tries = 0;
        loop {
            let left = self.limit.saturating_sub(began.elapsed()).max(Duration::from_secs(1));
            let endpoint = self.endpoint()?;
            let (turn, reply) = model::exchange(&endpoint, &body, left, self.record.as_deref(), &self.cancel, &mut *on)?;
            if self.cancel.fired() {
                return Ok(turn);
            }
            match decide(&turn, &reply, tries) {
                Next::Use => {
                    self.account(&turn);
                    return Ok(turn);
                }
                Next::Stop(why) => return Ok(Turn { finish: Finish::Error(why), ..turn }),
                Next::Wait(_, _) if began.elapsed() >= self.limit => return Ok(turn),
                Next::Wait(secs, what) => {
                    tries += 1;
                    on(Delta::Note(&format!("{what}; nothing from that try is used, and it goes again in {secs} seconds ({tries} of {})", TRIES - 1)));
                    if !self.wait(secs) {
                        return Ok(turn);
                    }
                }
            }
        }
    }
}

fn box_args(exe: &Path) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    let mut add = |items: &[&str]| a.extend(items.iter().map(OsString::from));
    add(&["--unshare-all", "--share-net", "--unshare-user", "--disable-userns", "--assert-userns-disabled"]);
    add(&["--die-with-parent", "--new-session", "--hostname", "writ", "--clearenv"]);
    add(&["--setenv", "PATH", "/bin", "--setenv", "HOME", "/tmp"]);
    for p in SYSTEM {
        add(&["--ro-bind", p, p]);
    }
    for p in NAMES {
        add(&["--ro-bind-try", p, p]);
    }
    add(&["--proc", "/proc", "--dev", "/dev"]);
    add(&["--size", &TMPFS_BYTES.to_string(), "--tmpfs", "/tmp"]);
    a.extend(["--ro-bind".into(), exe.into(), sandbox::INNER.into()]);
    a.extend(["--remount-ro".into(), "/".into(), "--chdir".into(), "/tmp".into()]);
    a
}

/// A command in the box a key's curl runs in, through the same second stage as any other.
fn in_box(argv: &[OsString]) -> Result<Command, String> {
    sandbox::trusted_bwrap()?;
    let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
    let mut c = Command::new(sandbox::BWRAP);
    c.env_clear().args(box_args(&exe)).arg(sandbox::INNER).arg("inner").args(["--write", "/tmp"]);
    for p in PORTS {
        c.arg("--tcp").arg(p.to_string());
    }
    c.arg("--").args(argv);
    Ok(c)
}

pub fn curl_in_box(flags: &[&str]) -> Result<Command, String> {
    let mut argv: Vec<OsString> = vec![model::CURL.into()];
    argv.extend(flags.iter().map(OsString::from));
    in_box(&argv)
}

/// `writ check`'s attacks from inside that box. What it says on its stdout is for `lines_of`.
pub fn attacks(env_text: &str, abstract_name: &str, absent: &[PathBuf]) -> Result<String, String> {
    let mut argv: Vec<OsString> = vec![sandbox::INNER.into(), "probe-net".into(), env_text.into(), abstract_name.into(), std::process::id().to_string().into()];
    argv.extend(absent.iter().map(|p| p.as_os_str().to_os_string()));
    let out = in_box(&argv)?.stdin(std::process::Stdio::null()).output().map_err(|e| format!("bwrap: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !text.lines().any(|l| l == "done") {
        return Err(format!("the attacks did not finish: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(text)
}

/// In the box, as `writ check` starts it.
pub fn probe(rest: &[OsString]) -> ! {
    use crate::probe::{broken, filter_refuses, leak, refused, skip, works};
    let [env_text, abstract_name, host_pid, absent @ ..] = rest else {
        eprintln!("probe-net: a value from the host's environment, an abstract socket, a host PID, paths");
        std::process::exit(inner::REFUSED_TO_START);
    };
    if unsafe { sys::prctl(sys::PR_GET_SECCOMP, 0i64, 0i64, 0i64, 0i64) } != 2 {
        eprintln!("probe-net runs only in a box");
        std::process::exit(inner::REFUSED_TO_START);
    }
    for p in absent {
        let name = format!("curl box: {} is not here", p.to_string_lossy());
        match fs::symlink_metadata(p) {
            Err(_) => refused(&name, "absent"),
            Ok(_) => leak(&name, "it exists"),
        }
    }
    for (name, path) in [
        ("curl box: /etc/passwd is not here", "/etc/passwd"),
        ("curl box: /etc/shadow is not here", "/etc/shadow"),
        ("curl box: /oath's catalog is not here", "/oath/objects"),
        ("curl box: /sys is not here", "/sys"),
    ] {
        match fs::symlink_metadata(path) {
            Err(_) => refused(name, "absent"),
            Ok(_) => leak(name, "it exists"),
        }
    }
    for (name, path) in [
        ("curl box: /bin cannot be written", "/bin/x"),
        ("curl box: /lib cannot be written", "/lib/x"),
        ("curl box: the store cannot be written", "/oath/store/x"),
        ("curl box: /etc cannot be written", "/etc/x"),
        ("curl box: the root cannot be written", "/x"),
    ] {
        match fs::write(path, "x") {
            Err(e) => refused(name, &e.to_string()),
            Ok(()) => leak(name, "it was written"),
        }
    }
    let names: Vec<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()).filter(|k| k != "PWD").collect();
    let wanted = env_text.to_string_lossy();
    let mut values = std::env::vars_os().any(|(_, v)| v.to_string_lossy().contains(&*wanted));
    // And in every process here, which bwrap's own environment would be, if it kept one.
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        if let Ok(b) = fs::read(entry.path().join("environ")) {
            values |= String::from_utf8_lossy(&b).contains(&*wanted);
        }
    }
    if values || names.iter().any(|k| !["PATH", "HOME"].contains(&k.as_str())) {
        leak("curl box: the environment is built from empty", &names.join(" "));
    } else {
        refused("curl box: the environment is built from empty", &names.join(" "));
    }
    let fds: Vec<String> = fs::read_dir("/proc/self/fd").map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    let others: Vec<String> = fds.iter().filter(|n| !["0", "1", "2"].contains(&n.as_str())).filter_map(|n| fs::read_link(format!("/proc/self/fd/{n}")).ok()).map(|p| p.display().to_string()).filter(|p| !p.starts_with("/proc/")).collect();
    if others.is_empty() {
        refused("curl box: no descriptor came in", "none");
    } else {
        leak("curl box: no descriptor came in", &others.join(" "));
    }
    let host_pid: i32 = host_pid.to_string_lossy().parse().unwrap_or(0);
    if fs::symlink_metadata(format!("/proc/{host_pid}")).is_ok() {
        leak("curl box: writ's own process is hidden", "it is in /proc");
    } else {
        refused("curl box: writ's own process is hidden", "absent");
    }
    let r = unsafe { sys::kill(host_pid, 0) };
    if r < 0 && sys::errno() == sys::ESRCH {
        refused("curl box: writ's own process cannot be signalled", "ESRCH");
    } else {
        leak("curl box: writ's own process cannot be signalled", &format!("returned {r}"));
    }
    if unsafe { sys::prctl(sys::PR_GET_NO_NEW_PRIVS, 0i64, 0i64, 0i64, 0i64) } == 1 {
        refused("curl box: no new privileges", "set");
    } else {
        leak("curl box: no new privileges", "not set");
    }
    match Command::new("/bin/sudo").args(["-n", "true"]).env_clear().output() {
        Ok(o) if o.status.success() => leak("curl box: sudo does not answer", "it ran"),
        Ok(_) => refused("curl box: sudo does not answer", "refused"),
        Err(e) => refused("curl box: sudo does not answer", &e.to_string()),
    }
    filter_refuses();

    // It shares the host's network, so this is what the kernel holds it to, where it can.
    let abi = landlock::abi();
    {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::net::{SocketAddr, UnixStream};
        let name = "curl box: a host abstract socket";
        let addr = SocketAddr::from_abstract_name(abstract_name.to_string_lossy().as_bytes()).expect("a name");
        match UnixStream::connect_addr(&addr) {
            Err(e) => refused(name, &e.to_string()),
            Ok(_) if abi < 6 => skip(name, "reached: this kernel's Landlock cannot scope one"),
            Ok(_) => leak(name, "connected"),
        }
    }
    if abi >= 4 {
        use std::io::ErrorKind::PermissionDenied;
        use std::net::{TcpListener, TcpStream};
        let at = |port: u16| std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match TcpStream::connect_timeout(&at(22), Duration::from_secs(2)) {
            Err(e) if e.kind() == PermissionDenied => refused("curl box: no TCP but HTTPS and DNS", &e.to_string()),
            other => leak("curl box: no TCP but HTTPS and DNS", &format!("port 22: {other:?}")),
        }
        match TcpListener::bind(at(0)) {
            Err(e) if e.kind() == PermissionDenied => refused("curl box: nothing can listen", &e.to_string()),
            other => leak("curl box: nothing can listen", &format!("{other:?}")),
        }
        match TcpStream::connect_timeout(&at(443), Duration::from_secs(2)) {
            Err(e) if e.kind() == PermissionDenied => broken("curl box: HTTPS may be tried", &e.to_string()),
            _ => works("curl box: HTTPS may be tried"),
        }
    } else {
        skip("curl box: no TCP but HTTPS and DNS", "this kernel's Landlock has no network rules");
    }
    match Command::new(model::CURL).args(["-q", "--version"]).env_clear().output() {
        Ok(o) if o.status.success() => works("curl box: curl runs"),
        other => broken("curl box: curl runs", &format!("{other:?}")),
    }
    match fs::read_to_string("/etc/resolv.conf") {
        Ok(_) => works("curl box: names can be resolved"),
        Err(e) => broken("curl box: names can be resolved", &e.to_string()),
    }
    if fs::write("/tmp/x", "x").is_ok() {
        works("curl box: /tmp can be written");
    } else {
        broken("curl box: /tmp can be written", "refused");
    }
    println!("done");
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Mind;
    use std::io::Write;
    use std::os::unix::net::UnixListener;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("writ-or-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn settings() -> Settings {
        Settings { model: "a/b".into(), context: 1000, max_output: 500, budget: 1.0 }
    }

    #[test]
    fn a_key_file_is_this_accounts_alone() {
        let home = temp("key");
        assert!(read_key(&home).unwrap_err().contains("writ key"));
        save_key(&home, "sk-or-v1-abc").unwrap();
        let path = key_path(&home);
        assert_eq!(read_key(&home).unwrap(), "sk-or-v1-abc");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_key(&home).unwrap_err().contains("chmod 600"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        // A link is refused, wherever it points.
        let real = home.join("real");
        fs::write(&real, "sk-or-v1-abc").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&real, &path).unwrap();
        assert!(read_key(&home).is_err());
        fs::remove_file(&path).unwrap();
        // What could not be a header or a line of config.
        for bad in ["", "two words", "a\"b", "a\\b", "a\nurl = http://attacker", "é"] {
            fs::write(&path, bad).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(read_key(&home).is_err(), "{bad:?}");
        }
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_key_file_holds_the_key_alone() {
        let home = temp("key-line");
        save_key(&home, "OPENROUTER_API_KEY=sk-or-v1-abc").unwrap();
        let why = read_key(&home).unwrap_err();
        assert!(why.contains("the key alone"), "{why}");
        // The file is a secret: what is said of it never has the line in it.
        assert!(!why.contains("sk-or-v1-abc") && !why.contains("OPENROUTER_API_KEY"), "{why}");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_choice_comes_back_as_it_went() {
        let home = temp("choice");
        assert!(matches!(chosen(&home), Ok(Choice::Local)));
        save_choice(&home, &Choice::OpenRouter(Settings { budget: 2.5, ..settings() })).unwrap();
        let Ok(Choice::OpenRouter(o)) = chosen(&home) else { panic!() };
        assert_eq!((o.model.as_str(), o.context, o.max_output, o.budget), ("a/b", 1000, 500, 2.5));
        save_choice(&home, &Choice::Local).unwrap();
        assert!(matches!(chosen(&home), Ok(Choice::Local)));
        fs::write(choice_path(&home), "{\"provider\":\"elsewhere\"}").unwrap();
        assert!(chosen(&home).is_err());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_models_name_cannot_steer_the_path() {
        for good in ["deepseek/deepseek-v4.1-flash-1017", "qwen/qwen3.5-9b", "z-ai/glm-4.7:floor"] {
            assert!(good_model(good), "{good}");
        }
        for bad in ["", "no-author", "a/b/c", "../etc/passwd", "a/..", "a/b?x=1", "a/b#f", "a b/c", "a/b c", "a/[1-9]", "a/{b,c}", ":x/y", "a:b/c", "a/b\n"] {
            assert!(!good_model(bad), "{bad:?}");
        }
    }

    const CATALOG: &str = r#"{"data":[
        {"model_id":"a/b","context_length":1000,"max_completion_tokens":100000,"pricing":{"prompt":"0.0000001","completion":"0.000001"},"supports_implicit_caching":true,"supported_parameters":["tools","max_tokens"]},
        {"model_id":"a/b","context_length":800,"max_completion_tokens":null,"pricing":{"prompt":"0.0000003","completion":"0.000002"},"supports_implicit_caching":false,"supported_parameters":["tools"]},
        {"model_id":"a/b","context_length":10,"max_completion_tokens":5,"pricing":{"prompt":"0","completion":"0"},"supported_parameters":["max_tokens"]},
        {"model_id":"a/small","context_length":1000,"max_completion_tokens":4096,"pricing":{"prompt":"0","completion":"0"},"supported_parameters":["tools"]},
        {"model_id":"a/text","context_length":1000,"pricing":{},"supported_parameters":["max_tokens"]}]}"#;

    #[test]
    fn what_a_catalog_says_of_a_model() {
        let c = json::parse(CATALOG).unwrap();
        let f = facts_of(&c, "a/b:floor").unwrap();
        // The route with no tools does not count, so nor does its tiny context.
        assert_eq!((f.routes, f.caching, f.context, f.max_output), (2, 1, 800, MAX_OUTPUT));
        assert!((f.prompt.0 - 0.1).abs() < 1e-9 && (f.prompt.1 - 0.3).abs() < 1e-9, "{f:?}");
        assert_eq!(facts_of(&c, "a/small").unwrap().max_output, 4096);
        assert!(facts_of(&c, "a/text").unwrap_err().contains("offers tools"));
        assert!(facts_of(&c, "a/none").unwrap_err().contains("keep nothing"));
    }

    fn failed(error: &str) -> Turn {
        Turn {
            content: String::new(),
            reasoning: String::new(),
            reasoning_details: None,
            calls: vec![],
            finish: Finish::Error("no".into()),
            usage: None,
            error: (!error.is_empty()).then(|| json::parse(error).unwrap()),
        }
    }

    fn reply(status: u16, retry_after: Option<u64>) -> Reply {
        Reply { status: Some(status), retry_after, exit: Some(22) }
    }

    fn waits(n: &Next) -> bool {
        matches!(n, Next::Wait(_, _))
    }

    #[test]
    fn what_is_tried_again_and_what_is_not() {
        let t = failed("");
        // A status alone.
        for status in [408, 429, 502, 503] {
            assert!(waits(&decide(&t, &reply(status, None), 0)), "{status}");
        }
        for status in [400, 401, 402, 403, 404, 422] {
            assert!(matches!(decide(&t, &reply(status, None), 0), Next::Stop(_)), "{status}");
        }
        assert!(decide(&t, &reply(401, None), 0) != Next::Stop("no".into()), "a refused key says what to do");
        // The stable field, over the status, and a limit somebody set, over both.
        let kind = |k: &str, code: u16| failed(&format!(r#"{{"code":{code},"message":"x","metadata":{{"error_type":"{k}"}}}}"#));
        assert!(waits(&decide(&kind("rate_limit_exceeded", 429), &Reply::default(), 0)));
        assert!(waits(&decide(&kind("provider_overloaded", 200), &Reply::default(), 0)));
        assert!(matches!(decide(&kind("context_length_exceeded", 502), &Reply::default(), 0), Next::Stop(_)));
        let limit = failed(r#"{"code":429,"message":"x","metadata":{"limit_source":"openrouter_key_limit"}}"#);
        assert!(matches!(decide(&limit, &reply(429, Some(1)), 0), Next::Stop(_)));
        // One limit does say when to come back.
        let flight = failed(r#"{"code":402,"message":"x","metadata":{"limit_source":"openrouter_in_flight_budget"}}"#);
        assert_eq!(decide(&flight, &reply(402, Some(3)), 0), Next::Wait(3, "OpenRouter said 402".into()));
        assert!(matches!(decide(&flight, &reply(402, None), 0), Next::Stop(_)));
        // A connection lost, and a stream cut off with no reason given. Not a failure curl cannot name.
        let lost = Reply { status: None, retry_after: None, exit: Some(7) };
        assert!(waits(&decide(&t, &lost, 0)));
        assert!(matches!(decide(&t, &Reply { exit: Some(3), ..lost.clone() }, 0), Next::Stop(_)));
        let cut = Turn { finish: Finish::CutOff, ..failed("") };
        assert!(waits(&decide(&cut, &Reply { exit: Some(0), ..lost }, 0)));
    }

    #[test]
    fn it_waits_as_asked_for_as_long_as_is_reasonable_and_not_forever() {
        let t = failed("");
        assert_eq!(decide(&t, &reply(429, Some(30)), 0), Next::Wait(30, "OpenRouter said 429".into()));
        let waited: Vec<u64> = (0..3).filter_map(|i| if let Next::Wait(s, _) = decide(&t, &reply(503, None), i) { Some(s) } else { None }).collect();
        assert_eq!(waited, BACKOFF_SECS);
        assert!(matches!(decide(&t, &reply(503, None), TRIES - 1), Next::Stop(_)));
        assert!(matches!(decide(&t, &reply(429, Some(MAX_WAIT_SECS + 1)), 0), Next::Stop(_)));
    }

    #[test]
    fn what_finished_is_used_whatever_it_says() {
        for finish in [Finish::Stop, Finish::ToolCalls, Finish::Length, Finish::ContentFilter, Finish::Other("x".into())] {
            let t = Turn { finish, ..failed("") };
            assert_eq!(decide(&t, &Reply::default(), 0), Next::Use);
        }
    }

    /// One reply for each connection, whatever was asked, and what was asked.
    fn server(path: &Path, replies: Vec<&'static str>) -> std::thread::JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let mut asked = Vec::new();
            for r in replies {
                let (mut c, _) = listener.accept().unwrap();
                let mut got = Vec::new();
                let mut buf = [0u8; 65536];
                let body = loop {
                    let n = c.read(&mut buf).unwrap();
                    got.extend_from_slice(&buf[..n]);
                    let Some(end) = got.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&got[..end]).to_lowercase();
                    let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length: ")).unwrap().trim().parse().unwrap();
                    if got.len() >= end + 4 + len {
                        break String::from_utf8_lossy(&got[end + 4..end + 4 + len]).into_owned();
                    }
                };
                let (status, rest) = r.split_once('|').unwrap();
                let (headers, stream) = rest.split_once('|').unwrap();
                c.write_all(format!("HTTP/1.1 {status} X\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{stream}", stream.len()).as_bytes()).unwrap();
                asked.push(body);
            }
            asked
        })
    }

    const GOOD: &str = "200||data: {\"provider\":\"Host\",\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1,\"cost\":0.25}}\n\ndata: [DONE]\n\n";
    const NO_COST: &str = "200||data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    const RATE: &str = "429|Retry-After: 1\r\n|{\"error\":{\"code\":429,\"message\":\"slow down\"}}";
    const BAD_KEY: &str = "401||{\"error\":{\"code\":401,\"message\":\"No auth credentials found\"}}";
    /// A failure after the status, with words already said.
    const MID: &str = "200||data: {\"choices\":[{\"delta\":{\"content\":\"half\"}}]}\n\ndata: {\"error\":{\"code\":502,\"message\":\"gone\"},\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"error\"}]}\n\n";

    fn hosted(dir: &Path, socket: PathBuf, budget: f64) -> Hosted {
        let mut h = Hosted::for_session(Settings { budget, ..settings() }, dir, "s1", dir, Duration::from_secs(30), Cancel::default(), &[]);
        h.fake = Some(socket);
        h
    }

    fn run_turn(h: &mut Hosted) -> (Turn, Vec<String>, String) {
        let (mut notes, mut said) = (Vec::new(), String::new());
        let turn = h
            .turn(&[chat::message("user", "go")], 100, &mut |d| match d {
                Delta::Note(t) => notes.push(t.to_string()),
                Delta::Content(t) => said.push_str(t),
                Delta::Reasoning(_) => {}
            })
            .unwrap();
        (turn, notes, said)
    }

    #[test]
    fn what_each_request_asks_for() {
        let d = temp("body");
        let path = d.join("s");
        let srv = server(&path, vec![GOOD]);
        let mut h = hosted(&d, path, 1.0);
        let (turn, _, _) = run_turn(&mut h);
        assert_eq!(turn.finish, Finish::Stop);
        let asked = srv.join().unwrap();
        let v = json::parse(&asked[0]).unwrap();
        assert_eq!(v.get("model").and_then(|m| m.str()), Some("a/b"));
        assert_eq!(v.get("session_id").and_then(|m| m.str()), Some("s1"));
        assert_eq!(v.get("max_tokens").and_then(|m| m.u64()), Some(100));
        assert!(v.get("stream_options").is_none());
        let p = v.get("provider").unwrap();
        assert_eq!(p.get("zdr"), Some(&Value::Bool(true)));
        assert_eq!(p.get("require_parameters"), Some(&Value::Bool(true)));
        assert_eq!(p.get("data_collection").and_then(|x| x.str()), Some("deny"));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_rate_limit_is_waited_out_and_the_person_told() {
        let d = temp("rate");
        let path = d.join("s");
        let srv = server(&path, vec![RATE, GOOD]);
        let mut h = hosted(&d, path, 1.0);
        let began = Instant::now();
        let (turn, notes, said) = run_turn(&mut h);
        assert_eq!(turn.finish, Finish::Stop);
        assert_eq!(said, "hi");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("429") && notes[0].contains("1 seconds"), "{notes:?}");
        assert!(began.elapsed() >= Duration::from_secs(1));
        assert_eq!(srv.join().unwrap().len(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_refused_key_is_not_tried_again() {
        let d = temp("refused");
        let path = d.join("s");
        // Only one reply: a second try would hang this and fail on its timeout.
        let srv = server(&path, vec![BAD_KEY]);
        let mut h = hosted(&d, path, 1.0);
        let (turn, notes, _) = run_turn(&mut h);
        let Finish::Error(e) = &turn.finish else { panic!("{:?}", turn.finish) };
        assert!(e.contains("No auth credentials found") && e.contains("writ key"), "{e}");
        assert!(notes.is_empty());
        assert!(turn.runnable().is_err());
        assert_eq!(srv.join().unwrap().len(), 1);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_stream_that_fails_partway_is_asked_again_and_only_the_good_one_counts() {
        let d = temp("mid");
        let path = d.join("s");
        let srv = server(&path, vec![MID, GOOD]);
        let mut h = hosted(&d, path, 1.0);
        let (turn, notes, said) = run_turn(&mut h);
        assert_eq!(turn.finish, Finish::Stop);
        assert_eq!(turn.content, "hi", "the partial turn is gone from what the model will be told it said");
        assert_eq!(said, "halfhi", "what the person saw stays where it was");
        assert!(notes[0].contains("nothing from that try is used"), "{notes:?}");
        assert_eq!(spent(&d), 0.25);
        srv.join().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_stop_during_a_wait_ends_it_now() {
        let d = temp("stop");
        let path = d.join("s");
        let srv = server(&path, vec!["429|Retry-After: 200\r\n|{\"error\":{\"code\":429,\"message\":\"slow\"}}"]);
        let mut h = hosted(&d, path, 1.0);
        let cancel = h.cancel.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            cancel.fire();
        });
        let began = Instant::now();
        let (turn, _, _) = run_turn(&mut h);
        assert!(began.elapsed() < Duration::from_secs(10));
        assert!(turn.runnable().is_err());
        t.join().unwrap();
        srv.join().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_budget_stops_a_session_and_survives_it() {
        let d = temp("budget");
        let path = d.join("s");
        let srv = server(&path, vec![GOOD, GOOD]);
        let mut h = hosted(&d, path.clone(), 0.5);
        assert_eq!(h.out_of_budget(), None);
        run_turn(&mut h);
        assert_eq!(h.out_of_budget(), None);
        run_turn(&mut h);
        assert!(h.out_of_budget().unwrap().contains("$0.50 of its $0.50"), "{:?}", h.out_of_budget());
        srv.join().unwrap();
        // The same session later, even after starting over.
        assert_eq!(spent(&d), 0.5);
        assert!(hosted(&d, path.clone(), 0.5).out_of_budget().is_some());
        // More budget lets it go on.
        assert_eq!(hosted(&d, path, 0.75).out_of_budget(), None);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_turn_that_does_not_say_its_cost_stops_the_session() {
        let d = temp("nocost");
        let path = d.join("s");
        let srv = server(&path, vec![NO_COST]);
        let mut h = hosted(&d, path, 100.0);
        run_turn(&mut h);
        assert!(h.out_of_budget().unwrap().contains("did not say"));
        srv.join().unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_session_with_nothing_spent_reads_as_nothing() {
        let d = temp("empty");
        assert_eq!(spent(&d), 0.0);
        for junk in ["junk", "-3", "NaN", "inf"] {
            fs::write(d.join(SPENT_NAME), junk).unwrap();
            assert_eq!(spent(&d), 0.0, "{junk}");
        }
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn dollars_read_as_dollars() {
        assert_eq!(dollars(1.0), "$1.00");
        assert_eq!(dollars(0.25), "$0.25");
        assert_eq!(dollars(0.0012), "$0.0012");
        assert_eq!(dollars(0.0), "$0.00");
        assert_eq!(range((0.05, 0.05)), "$0.050");
        assert_eq!(range((0.05, 1.8)), "$0.050–$1.80");
    }

    #[test]
    fn the_box_has_the_network_and_nothing_of_homes() {
        let a: Vec<String> = box_args(Path::new("/x/writ")).into_iter().map(|s| s.to_string_lossy().into_owned()).collect();
        let text = a.join(" ");
        for need in ["--unshare-all --share-net", "--clearenv", "--disable-userns", "--ro-bind-try /etc/resolv.conf", "--ro-bind /x/writ /writ", "--remount-ro /"] {
            assert!(text.contains(need), "{need}: {text}");
        }
        for bad in ["/home", "/etc/passwd", "/oath/run", "/oath/log", "--bind", "--dev-bind", "--cap-add", " / /"] {
            assert!(!text.contains(bad), "{bad}: {text}");
        }
    }
}
