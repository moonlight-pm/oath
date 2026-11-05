//! Reaching a model. A `curl` child does the talking, so writ has no TLS or HTTP of its own, and
//! a key lives in that child and here only: it goes in on curl's stdin, never in an argument or
//! the environment. A local server and OpenRouter differ by a few lines of curl's config, and by
//! where curl runs: for OpenRouter, in a box of its own (`openrouter::curl_in_box`).

use crate::chat::{self, Assembler, Delta, Turn};
use crate::{json, openrouter, sys};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const CURL: &str = "/bin/curl";
/// curl refuses a longer line of config, and the body is one line.
const MAX_BODY: usize = 8 << 20;
/// The one host a key goes to. Local servers and OpenRouter share the path after this.
const OPENROUTER: &str = "https://openrouter.ai/api";
/// A catalog of every model is about a megabyte.
const MAX_FETCH: u64 = 16 << 20;
/// How curl says what the program needs to know of a reply, apart from its bytes and its words.
const SAID: &str = "writ-reply";
const WRITE_OUT: &str = "write-out = \"%{stderr}\\nwrit-reply %{http_code} %header{retry-after}\\n\"\n";
/// OpenRouter sends a comment every so often while a model is slow, so this long with nothing at all
/// is a connection that has died.
const SILENT_SECS: u64 = 300;

/// Stops what is running for the person who asked for it: a model's turn, a tool in its box.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn fire(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn fired(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

pub enum Endpoint {
    /// A server of ours on a Unix socket: `writ serve`.
    Local(PathBuf),
    /// OpenRouter, and the key, which only its catalog does without.
    OpenRouter(Option<String>),
    /// A test's server on a socket, and a key to send it.
    #[cfg(test)]
    Fake(PathBuf, String),
}

/// What curl said of a reply: its status, how long it asks to be left alone, and how curl ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    pub status: Option<u16>,
    pub retry_after: Option<u64>,
    pub exit: Option<i32>,
}

/// Quote a value for a curl config file.
fn quote(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Where a request goes, as curl's config. Nothing but a path comes from the caller, and it follows
/// a host fixed here, so no answer can send a key anywhere else.
fn place(endpoint: &Endpoint, path: &str) -> Result<String, String> {
    if !path.starts_with('/') {
        return Err(format!("{path:?} is not a path"));
    }
    let on_socket = |socket: &Path| -> Result<String, String> {
        let at = socket.to_str().ok_or("the socket's path is not UTF-8")?;
        Ok(format!("unix-socket = {}\nurl = {}\nproto = \"=http\"\n", quote(at), quote(&format!("http://localhost{path}"))))
    };
    let mut c = match endpoint {
        Endpoint::Local(socket) => on_socket(socket)?,
        Endpoint::OpenRouter(key) => {
            let mut c = format!("url = {}\nproto = \"=https\"\ntlsv1.2\ngloboff\nconnect-timeout = 30\n", quote(&format!("{OPENROUTER}{path}")));
            if let Some(key) = key {
                c.push_str(&format!("header = {}\n", quote(&format!("Authorization: Bearer {key}"))));
            }
            c
        }
        #[cfg(test)]
        Endpoint::Fake(socket, key) => format!("{}header = {}\n", on_socket(socket)?, quote(&format!("Authorization: Bearer {key}"))),
    };
    c.push_str("max-redirs = 0\n");
    Ok(c)
}

/// What curl reads from its stdin. `@` first in a body would read a file, so a body starts with
/// `{` or nothing goes out.
fn config(endpoint: &Endpoint, body: &str, limit: Duration) -> Result<String, String> {
    if !body.starts_with('{') {
        return Err("a request body is an object".into());
    }
    if body.len() > MAX_BODY {
        return Err(format!("a request over {MAX_BODY} bytes"));
    }
    let mut c = place(endpoint, "/v1/chat/completions")?;
    c.push_str(&format!("max-time = {}\n", limit.as_secs()));
    c.push_str(WRITE_OUT);
    if matches!(endpoint, Endpoint::OpenRouter(_)) {
        c.push_str(&format!("speed-limit = 1\nspeed-time = {SILENT_SECS}\n"));
    }
    c.push_str("header = \"Content-Type: application/json\"\n");
    c.push_str("header = \"Accept: text/event-stream\"\n");
    c.push_str(&format!("data-binary = {}\n", quote(body)));
    Ok(c)
}

/// curl, somewhere it can reach this endpoint, with its stdin holding its orders, and what it says
/// on stderr being read.
fn start_curl(endpoint: &Endpoint, flags: &[&str], config: &str) -> Result<(std::process::Child, std::thread::JoinHandle<String>), String> {
    let mut command = match endpoint {
        Endpoint::OpenRouter(_) => openrouter::curl_in_box(flags)?,
        _ => {
            let mut c = Command::new(CURL);
            c.env_clear().args(flags);
            c
        }
    };
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{CURL}: {e}"))?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let errors = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.by_ref().take(64 << 10).read_to_end(&mut s);
        String::from_utf8_lossy(&s).trim().to_string()
    });
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    if let Err(e) = stdin.write_all(config.as_bytes()) {
        // It never read its orders: a box that would not start says why.
        let _ = child.kill();
        let _ = child.wait();
        let said = errors.join().unwrap_or_default();
        return Err(if said.is_empty() { format!("curl's stdin: {e}") } else { said });
    }
    Ok((child, errors))
}

/// What curl wrote on stderr: the reply's facts, and the rest, which is for a person.
fn read_said(said: &str) -> (Option<u16>, Option<u64>, String) {
    let (mut status, mut retry_after) = (None, None);
    let mut rest = Vec::new();
    for line in said.lines() {
        match line.strip_prefix(SAID).and_then(|l| l.strip_prefix(' ')) {
            Some(facts) => {
                let mut words = facts.split_whitespace();
                status = words.next().and_then(|s| s.parse().ok()).filter(|s| *s != 0);
                retry_after = words.next().and_then(|s| s.parse().ok());
            }
            None if !line.trim().is_empty() => rest.push(line),
            None => {}
        }
    }
    (status, retry_after, rest.join("\n"))
}

/// Send one request and read one turn back.
pub fn send<F: FnMut(Delta)>(endpoint: &Endpoint, body: &str, limit: Duration, record: Option<&Path>, cancel: &Cancel, on: F) -> Result<Turn, String> {
    Ok(exchange(endpoint, body, limit, record, cancel, on)?.0)
}

/// One request and one turn back, with what curl said of the reply. What arrives goes to `on` as it
/// comes, and to `record` as bytes, so a stream can be replayed in a test.
pub fn exchange<F: FnMut(Delta)>(
    endpoint: &Endpoint,
    body: &str,
    limit: Duration,
    record: Option<&Path>,
    cancel: &Cancel,
    mut on: F,
) -> Result<(Turn, Reply), String> {
    let config = config(endpoint, body, limit)?;
    // -q first: no ~/.curlrc. No environment: no proxy variables. Failing with the body keeps
    // a server's own reason for an error.
    let (mut child, errors) = start_curl(endpoint, &["-q", "--silent", "--show-error", "--no-buffer", "--fail-with-body", "--config", "-"], &config)?;
    let mut file = match record {
        Some(p) => Some(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut assembler = Assembler::default();
    let mut buf = [0u8; 16384];
    let mut stopped = false;
    loop {
        if cancel.fired() {
            // Its going is what tells the server to stop working on this.
            let _ = child.kill();
            stopped = true;
            break;
        }
        if !sys::wait_readable(stdout.as_raw_fd(), 100) {
            continue;
        }
        let n = stdout.read(&mut buf).map_err(|e| format!("curl's stdout: {e}"))?;
        if n == 0 {
            break;
        }
        if let Some(f) = &mut file {
            f.write_all(&buf[..n]).map_err(|e| format!("recording: {e}"))?;
        }
        assembler.push(&buf[..n], &mut on);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let (http, retry_after, said) = read_said(&errors.join().unwrap_or_default());
    let transport = (!status.success()).then(|| match status.code() {
        _ if stopped => "interrupted".to_string(),
        Some(c) if !said.is_empty() => format!("curl exited with {c}: {said}"),
        Some(c) => format!("curl exited with {c}"),
        None => "curl was killed".to_string(),
    });
    Ok((assembler.finish(transport), Reply { status: http, retry_after, exit: status.code() }))
}

/// One small GET: what the server answered, and its status. No answer at all is the error.
pub fn get(endpoint: &Endpoint, path: &str) -> Result<(u16, Vec<u8>), String> {
    let time = if matches!(endpoint, Endpoint::OpenRouter(_)) { 60 } else { 10 };
    let c = format!("{}max-time = {time}\n{WRITE_OUT}", place(endpoint, path)?);
    let (mut child, errors) = start_curl(endpoint, &["-q", "--silent", "--show-error", "--config", "-"], &c)?;
    let mut out = Vec::new();
    child.stdout.take().ok_or("no stdout")?.take(MAX_FETCH).read_to_end(&mut out).map_err(|e| e.to_string())?;
    let ended = child.wait().map_err(|e| e.to_string())?;
    let (status, _, said) = read_said(&errors.join().unwrap_or_default());
    match status {
        Some(s) if ended.success() => Ok((s, out)),
        _ => Err(if said.is_empty() { format!("nothing answered {path}") } else { said }),
    }
}

/// One small GET to the server: its body, or nothing if it said anything but yes.
fn fetch(endpoint: &Endpoint, path: &str) -> Result<Vec<u8>, String> {
    match get(endpoint, path)? {
        (200, body) => Ok(body),
        _ => Err(format!("the server did not answer {path}")),
    }
}

/// What the server says about itself: its context's size, for one.
pub fn props(endpoint: &Endpoint) -> Result<json::Value, String> {
    json::parse(&String::from_utf8_lossy(&fetch(endpoint, "/props")?))
}

/// Whether the model is loaded: the server says no while it loads.
pub fn ready(endpoint: &Endpoint) -> bool {
    fetch(endpoint, "/health").is_ok()
}

/// A recorded stream, read as if it were arriving.
pub fn replay<F: FnMut(Delta)>(path: &Path, mut on: F) -> Result<Turn, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut a = Assembler::default();
    a.push(&bytes, &mut on);
    Ok(a.finish(None))
}

/// One read-only tool, to see whether a model calls tools at all.
pub const READ_FILE: chat::Tool = chat::Tool {
    name: "read_file",
    description: "Read a text file from the project.",
    parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"The file's path, relative to the project."}},"required":["path"]}"#,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_cannot_read_a_file_and_stays_one_line() {
        let e = Endpoint::Local(PathBuf::from("/s"));
        let t = Duration::from_secs(9);
        assert!(config(&e, "@/home/.ssh/id_ed25519", t).is_err());
        assert!(config(&e, "x", t).is_err());
        let c = config(&e, "{\"a\":\"\\\"q\\\" \\\\ é\"}", t).unwrap();
        let line = c.lines().find(|l| l.starts_with("data-binary")).unwrap();
        assert_eq!(line, "data-binary = \"{\\\"a\\\":\\\"\\\\\\\"q\\\\\\\" \\\\\\\\ é\\\"}\"");
    }

    #[test]
    fn a_config_option_cannot_be_injected_through_a_value() {
        let e = Endpoint::Local(PathBuf::from("/s\nurl = \"http://attacker\""));
        let c = config(&e, "{\"a\":1}", Duration::from_secs(9)).unwrap();
        assert_eq!(c.matches("\nurl = ").count(), 1, "{c}");
        let body = "{\"a\":\"x\ny = 1\"}";
        let c = config(&Endpoint::Local(PathBuf::from("/s")), body, Duration::from_secs(9)).unwrap();
        assert!(!c.contains("\ny = 1"));
    }

    #[test]
    fn a_body_too_long_for_curl_is_refused() {
        let body = format!("{{\"a\":\"{}\"}}", "x".repeat(MAX_BODY));
        assert!(config(&Endpoint::Local(PathBuf::from("/s")), &body, Duration::from_secs(9)).is_err());
    }

    /// Against a real curl: a server on a socket here, and a body with everything that could
    /// go wrong in quoting. The server is this test; nothing leaves the machine.
    #[test]
    fn real_curl_sends_the_body_unchanged_and_reads_the_stream() {
        let d = std::env::temp_dir().join(format!("writ-model-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("s.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 65536];
            let body = loop {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                let Some(end) = got.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                let head = String::from_utf8_lossy(&got[..end]).to_lowercase();
                let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length: ")).unwrap().trim().parse().unwrap();
                if got.len() >= end + 4 + len {
                    break got[end + 4..end + 4 + len].to_vec();
                }
            };
            let stream = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{stream}", stream.len());
            s.write_all(reply.as_bytes()).unwrap();
            body
        });
        let sent = "{\"t\":\"quote \\\" slash \\\\ newline \\n tab \\t unicode é你🙂 at @file\"}";
        let mut seen = String::new();
        let turn = send(&Endpoint::Local(path), sent, Duration::from_secs(20), None, &Cancel::default(), |d| {
            if let Delta::Content(t) = d {
                seen.push_str(t)
            }
        })
        .unwrap();
        assert_eq!(String::from_utf8(server.join().unwrap()).unwrap(), sent);
        assert_eq!(seen, "hi");
        assert_eq!(turn.finish, chat::Finish::Stop);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_key_goes_in_the_config_with_one_host_and_https_only() {
        let key = "sk-or-v1-abc".to_string();
        let c = config(&Endpoint::OpenRouter(Some(key)), "{}", Duration::from_secs(9)).unwrap();
        assert!(c.contains("header = \"Authorization: Bearer sk-or-v1-abc\"\n"), "{c}");
        assert!(c.contains("url = \"https://openrouter.ai/api/v1/chat/completions\"\n"), "{c}");
        assert!(c.contains("proto = \"=https\"\n") && c.contains("max-redirs = 0\n"), "{c}");
        assert_eq!(c.matches("\nurl = ").count() + usize::from(c.starts_with("url = ")), 1, "{c}");
        // A key cannot carry another line of config, should a file ever hold one.
        let c = config(&Endpoint::OpenRouter(Some("k\nurl = \"http://attacker\"".to_string())), "{}", Duration::from_secs(9)).unwrap();
        assert!(!c.contains("\nurl = \"http://attacker"), "{c}");
        let c = place(&Endpoint::OpenRouter(None), "/v1/models").unwrap();
        assert!(!c.contains("Authorization"), "{c}");
        assert!(place(&Endpoint::OpenRouter(None), "@evil").is_err());
    }

    #[test]
    fn what_curl_says_of_a_reply_is_apart_from_what_it_says_to_a_person() {
        assert_eq!(read_said("writ-reply 200 "), (Some(200), None, String::new()));
        assert_eq!(read_said("curl: (22) nope\n\nwrit-reply 429 7"), (Some(429), Some(7), "curl: (22) nope".to_string()));
        assert_eq!(read_said("curl: (7) refused\n\nwrit-reply 000 "), (None, None, "curl: (7) refused".to_string()));
        assert_eq!(read_said("writ-reply 503 Wed, 21 Oct 2026 07:28:00 GMT"), (Some(503), None, String::new()));
    }

    /// A server here, a real curl, and what it would take to see the key from outside.
    #[test]
    fn a_key_reaches_the_server_and_no_argument_or_environment() {
        let d = std::env::temp_dir().join(format!("writ-key-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("s.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let key = "sk-or-v1-writ-test-key".to_string();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            // Who is on the other end, while they are.
            #[repr(C)]
            #[derive(Default)]
            struct Ucred {
                pid: i32,
                uid: u32,
                gid: u32,
            }
            extern "C" {
                fn getsockopt(fd: i32, level: i32, name: i32, value: *mut Ucred, len: *mut u32) -> i32;
            }
            let (mut peer, mut len) = (Ucred::default(), std::mem::size_of::<Ucred>() as u32);
            let got = unsafe { getsockopt(s.as_raw_fd(), 1, 17, &mut peer, &mut len) };
            assert_eq!(got, 0);
            let seen = |file: &str| std::fs::read(format!("/proc/{}/{file}", peer.pid)).unwrap();
            let (argv, env) = (seen("cmdline"), seen("environ"));
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            let stream = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{stream}", stream.len()).as_bytes()).unwrap();
            (String::from_utf8_lossy(&got).into_owned(), argv, env)
        });
        let e = Endpoint::Fake(path, key.clone());
        let (turn, reply) = exchange(&e, "{}", Duration::from_secs(20), None, &Cancel::default(), |_| {}).unwrap();
        let (head, argv, env) = server.join().unwrap();
        assert!(head.contains(&format!("Bearer {key}")), "{head}");
        assert!(String::from_utf8_lossy(&argv).contains("curl"));
        for seen in [argv, env] {
            assert!(!String::from_utf8_lossy(&seen).contains(&key));
        }
        assert_eq!(turn.finish, chat::Finish::Stop);
        assert_eq!(reply, Reply { status: Some(200), retry_after: None, exit: Some(0) });
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn no_server_is_an_error_with_a_reason() {
        let e = Endpoint::Local(std::env::temp_dir().join("writ-no-such.sock"));
        let t = send(&e, "{}", Duration::from_secs(5), None, &Cancel::default(), |_| {}).unwrap();
        assert!(matches!(t.finish, chat::Finish::Error(_)));
        assert!(t.runnable().is_err());
    }
}
