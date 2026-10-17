use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

use oath_core::{fetch_adv, is_realization_id, read_advisories, write_advisories, PkgAdvisory};

static PATH_LOCK: Mutex<()> = Mutex::new(());

const H: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Fetches run curl (R18); one test at a time touches PATH-found tools.
fn with_fetch<T>(f: impl FnOnce() -> T) -> T {
    let _guard = PATH_LOCK.lock().unwrap();
    f()
}

fn serve_status(status: &str, body: &[u8]) -> (String, std::thread::JoinHandle<()>) {
    let status = status.to_string();
    let body = body.to_vec();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let hdr = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    (format!("http://{addr}"), h)
}

#[test]
fn fetch_adv_writes_file() {
    let d = tempfile::tempdir().unwrap();
    assert!(is_realization_id(H));
    write_advisories(
        d.path(),
        "curl",
        H,
        vec![PkgAdvisory {
            id: "CVE-TEST-1".into(),
            url: "https://example.invalid/CVE-TEST-1".into(),
            summary: String::new(),
        }],
    )
    .unwrap();
    let body = std::fs::read(d.path().join("store/adv/curl").join(format!("{H}.json"))).unwrap();
    std::fs::remove_file(d.path().join("store/adv/curl").join(format!("{H}.json"))).unwrap();
    let (origin, _h) = serve_status("200 OK", &body);
    with_fetch(|| {
        fetch_adv(d.path(), "curl", &origin, H).unwrap();
    });
    let a = read_advisories(d.path(), "curl", H).expect("fetched");
    assert_eq!(a[0].id, "CVE-TEST-1");
}

#[test]
fn fetch_adv_failure_does_not_refuse() {
    let d = tempfile::tempdir().unwrap();
    let (origin, _h) = serve_status("404 Not Found", b"missing");
    with_fetch(|| {
        fetch_adv(d.path(), "curl", &origin, H).unwrap();
    });
    assert!(read_advisories(d.path(), "curl", H).is_none());
}
