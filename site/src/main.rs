//! oath.moonlight.pm: the product page and the manual as one static
//! binary. Every page is rendered once at start from files compiled in
//! (`docs/manual/*.md` among them), so an image serves the manual of the
//! commit it was built from. HTTP/1.1, GET and HEAD only, behind Wicket's
//! front door.

mod manual;
mod site;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use site::{Resource, Site};

/// The address the site answers on. A request for a former host is sent
/// here with the same path.
const CANONICAL: &str = "oath.moonlight.pm";
const FORMER: &[&str] = &["oath.wicket.cloud"];

/// Nothing runs script, embeds a frame or posts a form; styles, fonts
/// and images come from this site.
const CSP: &str = "default-src 'none'; style-src 'self'; font-src 'self'; img-src 'self'; \
                   base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const MAX_HEAD: usize = 16 * 1024;
const MAX_CONNECTIONS: usize = 256;
const IDLE: Duration = Duration::from_secs(30);

fn main() {
    // As PID 1 in a container, SIGTERM is ignored unless handled.
    unsafe { signal(SIGTERM, on_term) };

    let port = ["PORT", "WICKET_PORT_HTTP", "OATH_SITE_PORT"]
        .iter()
        .find_map(|k| std::env::var(k).ok()?.parse::<u16>().ok().filter(|p| *p > 0))
        .unwrap_or(8090);
    let site = Arc::new(Site::build());
    let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("oath-site: bind port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("oath-site: http://127.0.0.1:{port}/");

    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let (site, open) = (site.clone(), open.clone());
        std::thread::spawn(move || {
            let _ = serve(stream, &site);
            open.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

const SIGTERM: i32 = 15;

extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    fn _exit(status: i32) -> !;
}

extern "C" fn on_term(_: i32) {
    unsafe { _exit(0) }
}

struct Request {
    method: String,
    path: String,
    target: String,
    host: String,
    if_none_match: Option<String>,
    keep_alive: bool,
    has_body: bool,
}

/// Answer requests on one connection until the client closes it, sends
/// something this server does not take, or goes quiet for `IDLE`.
fn serve(stream: TcpStream, site: &Site) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IDLE))?;
    stream.set_write_timeout(Some(IDLE))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let head = match read_head(&mut reader)? {
            Some(h) => h,
            None => return Ok(()),
        };
        let Some(req) = parse(&head) else {
            respond(&mut out, false, 400, "Bad Request", &[], "text/plain; charset=utf-8", b"bad request\n")?;
            return Ok(());
        };
        let keep = req.keep_alive && !req.has_body;
        if req.method != "GET" && req.method != "HEAD" {
            let allow = [("Allow", "GET, HEAD".to_string())];
            respond(&mut out, false, 405, "Method Not Allowed", &allow, "text/plain; charset=utf-8", b"GET or HEAD\n")?;
            return Ok(());
        }
        let head_only = req.method == "HEAD";
        let host = req.host.split(':').next().unwrap_or("");
        if FORMER.contains(&host) {
            let to = [("Location", format!("https://{CANONICAL}{}", req.target))];
            respond(&mut out, keep, 301, "Moved Permanently", &to, "text/plain; charset=utf-8", b"")?;
        } else {
            let (status, reason, res) = match site.get(&req.path) {
                Some(r) => (200, "OK", r),
                None => (404, "Not Found", &site.missing),
            };
            send(&mut out, keep, status, reason, res, &req, head_only)?;
        }
        if !keep {
            return Ok(());
        }
    }
}

/// The request line and headers, up to the blank line. `None` when the
/// client closed the connection between requests.
fn read_head(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<String>> {
    let mut head = String::new();
    loop {
        let mut line = String::new();
        let n = reader.by_ref().take((MAX_HEAD - head.len()) as u64 + 1).read_line(&mut line)?;
        if n == 0 {
            return Ok(if head.is_empty() { None } else { Some(head) });
        }
        if head.len() + n > MAX_HEAD {
            return Err(std::io::Error::other("request head too large"));
        }
        if line == "\r\n" || line == "\n" {
            if head.is_empty() {
                continue; // a stray blank line before a request is allowed
            }
            return Ok(Some(head));
        }
        head.push_str(&line);
    }
}

fn parse(head: &str) -> Option<Request> {
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    if !target.starts_with('/') || !version.starts_with("HTTP/1.") {
        return None;
    }
    let mut req = Request {
        method: method.to_string(),
        path: target.split(['?', '#']).next().unwrap_or("/").to_string(),
        target: target.to_string(),
        host: String::new(),
        if_none_match: None,
        keep_alive: version == "HTTP/1.1",
        has_body: false,
    };
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => req.host = value.to_ascii_lowercase(),
            "if-none-match" => req.if_none_match = Some(value.to_string()),
            "connection" => {
                let v = value.to_ascii_lowercase();
                if v.contains("close") {
                    req.keep_alive = false;
                } else if v.contains("keep-alive") {
                    req.keep_alive = true;
                }
            }
            "content-length" => req.has_body |= value != "0",
            "transfer-encoding" => req.has_body = true,
            _ => {}
        }
    }
    Some(req)
}

/// A resource with its caching headers: a 304 when the client already
/// holds this ETag.
fn send(
    out: &mut TcpStream,
    keep: bool,
    status: u16,
    reason: &str,
    res: &Resource,
    req: &Request,
    head_only: bool,
) -> std::io::Result<()> {
    let headers = [("ETag", res.etag.clone()), ("Cache-Control", res.cache.to_string())];
    let fresh = status == 200
        && req.if_none_match.as_deref().is_some_and(|tags| {
            tags.split(',').any(|t| t.trim().trim_start_matches("W/") == res.etag || t.trim() == "*")
        });
    if fresh {
        return respond(out, keep, 304, "Not Modified", &headers, res.content_type, b"");
    }
    let body: &[u8] = if head_only { b"" } else { &res.body };
    write_response(out, keep, status, reason, &headers, res.content_type, res.body.len(), body)
}

fn respond(
    out: &mut TcpStream,
    keep: bool,
    status: u16,
    reason: &str,
    extra: &[(&str, String)],
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let len = if status == 304 { 0 } else { body.len() };
    write_response(out, keep, status, reason, extra, content_type, len, body)
}

#[allow(clippy::too_many_arguments)]
fn write_response(
    out: &mut TcpStream,
    keep: bool,
    status: u16,
    reason: &str,
    extra: &[(&str, String)],
    content_type: &str,
    length: usize,
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {reason}\r\n");
    if status != 304 {
        head.push_str(&format!("Content-Type: {content_type}\r\nContent-Length: {length}\r\n"));
    }
    if !extra.iter().any(|(k, _)| *k == "Cache-Control") {
        head.push_str("Cache-Control: no-cache\r\n");
    }
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Security-Policy: {CSP}\r\n"));
    head.push_str("X-Content-Type-Options: nosniff\r\n");
    head.push_str("Referrer-Policy: strict-origin-when-cross-origin\r\n");
    head.push_str("Strict-Transport-Security: max-age=31536000\r\n");
    head.push_str(if keep { "Connection: keep-alive\r\n\r\n" } else { "Connection: close\r\n\r\n" });
    out.write_all(head.as_bytes())?;
    out.write_all(body)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_request() {
        let r = parse("GET /manual?x=1 HTTP/1.1\r\nHost: Oath.Moonlight.pm\r\nIf-None-Match: \"ab\"\r\n").unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str(), r.host.as_str()), ("GET", "/manual", "oath.moonlight.pm"));
        assert_eq!(r.target, "/manual?x=1");
        assert_eq!(r.if_none_match.as_deref(), Some("\"ab\""));
        assert!(r.keep_alive && !r.has_body);
    }

    #[test]
    fn refuses_what_is_not_http() {
        assert!(parse("GET manual HTTP/1.1\r\n").is_none());
        assert!(parse("GET / SPDY/3\r\n").is_none());
        assert!(parse("GET / HTTP/1.1\r\nno colon here\r\n").is_none());
    }

    #[test]
    fn http_1_0_closes_and_a_body_is_noticed() {
        let r = parse("GET / HTTP/1.0\r\n").unwrap();
        assert!(!r.keep_alive);
        let r = parse("POST / HTTP/1.1\r\nContent-Length: 5\r\n").unwrap();
        assert!(r.has_body);
    }
}
