use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oath_core::{fetch_adv, is_realization_id, read_advisories, write_advisories, PkgAdvisory};

static PATH_LOCK: Mutex<()> = Mutex::new(());

const H: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p).find_map(|d| {
            let c = d.join(name);
            c.is_file().then_some(c)
        })
    })
}

fn with_wget_shim<T>(bin: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = PATH_LOCK.lock().unwrap();
    std::fs::create_dir_all(bin).unwrap();
    let wget = bin.join("wget");
    if which("wget").is_none() {
        let curl = which("curl").expect("curl to shim wget in host tests");
        std::fs::write(
            &wget,
            format!(
                "#!/bin/sh\n\
out=\n\
url=\n\
while [ $# -gt 0 ]; do\n\
  case \"$1\" in\n\
    -q) shift ;;\n\
    -O) out=$2; shift 2 ;;\n\
    -*) shift ;;\n\
    *) url=$1; shift ;;\n\
  esac\n\
done\n\
exec {} -sS -L --http1.1 -o \"$out\" \"$url\"\n",
                curl.display()
            ),
        )
        .unwrap();
        let mut p = std::fs::metadata(&wget).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&wget, p).unwrap();
    }
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old}", bin.display()));
    let r = f();
    std::env::set_var("PATH", old);
    r
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
fn fetch_adv_wget_writes_file() {
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
    let bin = d.path().join("bin");
    with_wget_shim(&bin, || {
        fetch_adv(d.path(), "curl", &origin, H).unwrap();
    });
    let a = read_advisories(d.path(), "curl", H).expect("fetched");
    assert_eq!(a[0].id, "CVE-TEST-1");
}

#[test]
fn fetch_adv_wget_failure_does_not_refuse() {
    let d = tempfile::tempdir().unwrap();
    let (origin, _h) = serve_status("404 Not Found", b"missing");
    let bin = d.path().join("bin");
    with_wget_shim(&bin, || {
        fetch_adv(d.path(), "curl", &origin, H).unwrap();
    });
    assert!(read_advisories(d.path(), "curl", H).is_none());
}
