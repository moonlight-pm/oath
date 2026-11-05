//! Reaching a model. A `curl` child does the talking, so writ has no TLS or HTTP of its own, and
//! a key will live in that child and here only: it goes in on curl's stdin, never in an argument
//! or the environment. A local server and OpenRouter differ by one line of curl's config.

use crate::chat::{self, Assembler, Delta, Turn};
use crate::json;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const CURL: &str = "/bin/curl";
/// curl refuses a longer line of config, and the body is one line.
const MAX_BODY: usize = 8 << 20;

pub enum Endpoint {
    /// A server of ours on a Unix socket: `writ serve`.
    Local(PathBuf),
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

/// What curl reads from its stdin. `@` first in a body would read a file, so a body starts with
/// `{` or nothing goes out.
fn config(endpoint: &Endpoint, body: &str, limit: Duration) -> Result<String, String> {
    if !body.starts_with('{') {
        return Err("a request body is an object".into());
    }
    if body.len() > MAX_BODY {
        return Err(format!("a request over {MAX_BODY} bytes"));
    }
    let mut c = String::new();
    match endpoint {
        Endpoint::Local(socket) => {
            let path = socket.to_str().ok_or("the socket's path is not UTF-8")?;
            c.push_str(&format!("unix-socket = {}\n", quote(path)));
            c.push_str("url = \"http://localhost/v1/chat/completions\"\n");
            c.push_str("proto = \"=http\"\n");
        }
    }
    c.push_str("max-redirs = 0\n");
    c.push_str(&format!("max-time = {}\n", limit.as_secs()));
    c.push_str("header = \"Content-Type: application/json\"\n");
    c.push_str("header = \"Accept: text/event-stream\"\n");
    c.push_str(&format!("data-binary = {}\n", quote(body)));
    Ok(c)
}

/// Send one request and read one turn back. What arrives goes to `on` as it comes, and to
/// `record` as bytes, so a stream can be replayed in a test.
pub fn send<F: FnMut(Delta)>(
    endpoint: &Endpoint,
    body: &str,
    limit: Duration,
    record: Option<&Path>,
    mut on: F,
) -> Result<Turn, String> {
    let config = config(endpoint, body, limit)?;
    // -q first: no ~/.curlrc. No environment: no proxy variables. Failing with the body keeps
    // a server's own reason for an error.
    let mut child = Command::new(CURL)
        .env_clear()
        .args(["-q", "--silent", "--show-error", "--no-buffer", "--fail-with-body", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{CURL}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    stdin.write_all(config.as_bytes()).map_err(|e| format!("curl's stdin: {e}"))?;
    drop(stdin);
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let errors = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.by_ref().take(64 << 10).read_to_end(&mut s);
        String::from_utf8_lossy(&s).trim().to_string()
    });
    let mut file = match record {
        Some(p) => Some(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut assembler = Assembler::default();
    let mut buf = [0u8; 16384];
    loop {
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
    let said = errors.join().unwrap_or_default();
    let transport = (!status.success()).then(|| match status.code() {
        Some(c) if !said.is_empty() => format!("curl exited with {c}: {said}"),
        Some(c) => format!("curl exited with {c}"),
        None => "curl was killed".to_string(),
    });
    Ok(assembler.finish(transport))
}

/// One small GET to the server: its body, or nothing if it said anything but yes.
fn fetch(endpoint: &Endpoint, path: &str) -> Result<Vec<u8>, String> {
    let Endpoint::Local(socket) = endpoint;
    let socket = socket.to_str().ok_or("the socket's path is not UTF-8")?;
    let c = format!("unix-socket = {}\nurl = \"http://localhost{path}\"\nproto = \"=http\"\nmax-redirs = 0\nmax-time = 10\n", quote(socket));
    let mut child = Command::new(CURL)
        .env_clear()
        .args(["-q", "--silent", "--show-error", "--fail", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{CURL}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    stdin.write_all(c.as_bytes()).map_err(|e| format!("curl's stdin: {e}"))?;
    drop(stdin);
    let mut out = Vec::new();
    child.stdout.take().ok_or("no stdout")?.take(4 << 20).read_to_end(&mut out).map_err(|e| e.to_string())?;
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err(format!("the server did not answer {path}"));
    }
    Ok(out)
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
        let turn = send(&Endpoint::Local(path), sent, Duration::from_secs(20), None, |d| {
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
    fn no_server_is_an_error_with_a_reason() {
        let e = Endpoint::Local(std::env::temp_dir().join("writ-no-such.sock"));
        let t = send(&e, "{}", Duration::from_secs(5), None, |_| {}).unwrap();
        assert!(matches!(t.finish, chat::Finish::Error(_)));
        assert!(t.runnable().is_err());
    }
}
