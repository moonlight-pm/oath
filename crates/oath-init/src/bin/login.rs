//! `svc:login` (R42): `home`'s password and the seat's owner, for the
//! desk's lock screen, its settings and the console.
//!
//! Root. Listens on `oath_core::login::SOCKET` for `home` and root
//! (SO_PEERCRED), one JSON request a line, one answer a line; the calls
//! are in `oath_core::login`. The password is an Argon2id hash (64 MiB,
//! three passes) in `PASSWORD_FILE`, mode 0600; none at first. Wrong
//! passwords are answered after a delay that doubles with each one in a
//! row (1 s, 2 s, … 30 s) and ends at a right one; checks wait their turn,
//! so the delay holds for every caller at once. Auto-login is
//! `host:local.autologin`, which it sets through `oath set` and `oath
//! apply` (as `svc:login` in the apply log). The seat is
//! `oath_core::login::SEAT_FILE`; a change asks PID 1 to converge.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use oath_core::login::{Seat, PASSWORD_FILE, SEAT_FILE, SOCKET};
use oath_core::{Catalog, ObjectId, DEFAULT_ROOT};
use serde_json::{json, Value};

/// A request line, at most.
const LINE_MAX: u64 = 8192;
/// A password, at most (bytes).
const PASSWORD_MAX: usize = 1024;
const DELAY_MAX: Duration = Duration::from_secs(30);

/// Wrong passwords in a row, and when the next check may run.
struct Throttle {
    wrong: u32,
    next: Instant,
}

fn main() {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("login: runs as root (svc:login)");
        std::process::exit(1);
    }
    let _ = fs::create_dir_all("/oath/run");
    let dir = Path::new(PASSWORD_FILE).parent().expect("parent");
    if let Err(e) = fs::create_dir_all(dir)
        .and_then(|_| fs::set_permissions(dir, fs::Permissions::from_mode(0o700)))
    {
        eprintln!("login: {}: {e}", dir.display());
        std::process::exit(1);
    }
    let _ = fs::remove_file(SOCKET);
    let listener = match UnixListener::bind(SOCKET) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("login: bind {SOCKET}: {e}");
            std::process::exit(1);
        }
    };
    // Anyone may connect; only home and root are answered.
    let _ = fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o666));
    eprintln!("login: listening on {SOCKET}");
    let throttle = Arc::new(Mutex::new(Throttle { wrong: 0, next: Instant::now() }));
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let throttle = Arc::clone(&throttle);
        std::thread::spawn(move || serve(conn, &throttle));
    }
}

fn serve(conn: UnixStream, throttle: &Mutex<Throttle>) {
    let uid = peer_uid(&conn);
    let mut out = match conn.try_clone() {
        Ok(c) => c,
        Err(_) => return,
    };
    if uid != Some(0) && uid != Some(oath_core::seat::UID) {
        let _ = answer(&mut out, &refuse("only home and root may ask svc:login"));
        return;
    }
    let _ = conn.set_read_timeout(Some(Duration::from_secs(60)));
    let mut lines = BufReader::new(conn);
    loop {
        let mut line = String::new();
        match (&mut lines).take(LINE_MAX).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if !line.ends_with('\n') && line.len() as u64 >= LINE_MAX {
            let _ = answer(&mut out, &refuse("request too long"));
            return;
        }
        let reply = match serde_json::from_str::<Value>(line.trim()) {
            Ok(req) => handle(&req, throttle),
            Err(e) => refuse(&format!("not JSON: {e}")),
        };
        if answer(&mut out, &reply).is_err() {
            return;
        }
        if reply.get("seat_changed") == Some(&json!(true)) {
            notify_init();
        }
    }
}

fn handle(req: &Value, throttle: &Mutex<Throttle>) -> Value {
    let text = |k: &str| req.get(k).and_then(Value::as_str);
    match text("call") {
        Some("status") => json!({
            "ok": true,
            "password": stored().is_some(),
            "autologin": autologin(),
            "seat": Seat::current().as_str(),
        }),
        Some("check") => {
            let Some(pw) = text("password") else { return refuse("check needs password") };
            match stored() {
                None => json!({ "ok": true, "right": false, "reason": "no password is set" }),
                Some(hash) => json!({ "ok": true, "right": check(&hash, pw, throttle) }),
            }
        }
        Some("set") => {
            let Some(new) = text("new") else { return refuse("set needs new (empty removes)") };
            if new.len() > PASSWORD_MAX {
                return refuse("the password is longer than 1024 bytes");
            }
            if let Some(hash) = stored() {
                let old = text("old").unwrap_or("");
                if !check(&hash, old, throttle) {
                    return refuse("wrong password");
                }
            }
            match set(new) {
                Ok(()) => {
                    eprintln!("login: password {}", if new.is_empty() { "removed" } else { "set" });
                    json!({ "ok": true })
                }
                Err(e) => refuse(&e),
            }
        }
        Some("autologin") => {
            let Some(on) = req.get("on").and_then(Value::as_bool) else {
                return refuse("autologin needs on (true or false)");
            };
            if let Some(hash) = stored() {
                if !check(&hash, text("password").unwrap_or(""), throttle) {
                    return refuse("wrong password");
                }
            }
            match set_autologin(on) {
                Ok(()) => json!({ "ok": true }),
                Err(e) => refuse(&e),
            }
        }
        Some("desk") => {
            let Some(up) = req.get("up").and_then(Value::as_bool) else {
                return refuse("desk needs up (true or false)");
            };
            let seat = if up { Seat::Desk } else { Seat::Console };
            match fs::write(SEAT_FILE, format!("{}\n", seat.as_str())) {
                Ok(()) => {
                    eprintln!("login: the {} has the seat", seat.as_str());
                    json!({ "ok": true, "seat": seat.as_str(), "seat_changed": true })
                }
                Err(e) => refuse(&format!("{SEAT_FILE}: {e}")),
            }
        }
        Some(other) => refuse(&format!("no call {other:?}: status, check, set, autologin, desk")),
        None => refuse("a request is {\"call\": …}"),
    }
}

/// Whether `pw` is the password. One check at a time: each waits out the
/// delay the last wrong one set, and a wrong one is answered after its own.
fn check(hash: &str, pw: &str, throttle: &Mutex<Throttle>) -> bool {
    let mut t = throttle.lock().unwrap_or_else(|p| p.into_inner());
    std::thread::sleep(t.next.saturating_duration_since(Instant::now()));
    let right = PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false);
    if right {
        t.wrong = 0;
        t.next = Instant::now();
        return true;
    }
    t.wrong += 1;
    let delay = Duration::from_secs(1 << (t.wrong - 1).min(5)).min(DELAY_MAX);
    t.next = Instant::now() + delay;
    eprintln!("login: wrong password ({} in a row)", t.wrong);
    std::thread::sleep(delay);
    false
}

/// The stored hash, if a password is set.
fn stored() -> Option<String> {
    let s = fs::read_to_string(PASSWORD_FILE).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Hash and store `new`, or remove the password when it is empty.
fn set(new: &str) -> Result<(), String> {
    if new.is_empty() {
        return match fs::remove_file(PASSWORD_FILE) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{PASSWORD_FILE}: {e}")),
        };
    }
    let mut salt = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut salt))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    let params = Params::new(64 * 1024, 3, 1, None).map_err(|e| e.to_string())?;
    let hash = Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(new.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();
    let tmp = format!("{PASSWORD_FILE}.new");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("{tmp}: {e}"))?;
    f.write_all(format!("{hash}\n").as_bytes())
        .and_then(|_| f.sync_all())
        .and_then(|_| fs::rename(&tmp, PASSWORD_FILE))
        .map_err(|e| format!("{PASSWORD_FILE}: {e}"))
}

/// `host:local.autologin` as applied (on when absent).
fn autologin() -> bool {
    let id = ObjectId::new("host", "local");
    let spec = Catalog::open(DEFAULT_ROOT).ok().and_then(|cat| {
        cat.applied(&id).ok().flatten().or_else(|| cat.get(&id).ok().map(|o| o.desired))
    });
    spec.and_then(|v| v.get("autologin").and_then(Value::as_bool)).unwrap_or(true)
}

/// Set and apply `host:local.autologin`. Refused while `host:local` has
/// other changes nobody applied: this apply would carry them too.
fn set_autologin(on: bool) -> Result<(), String> {
    let id = ObjectId::new("host", "local");
    let cat = Catalog::open(DEFAULT_ROOT).map_err(|e| e.to_string())?;
    let desired = cat.get(&id).map_err(|e| e.to_string())?.desired;
    if let Some(applied) = cat.applied(&id).map_err(|e| e.to_string())? {
        let other: Vec<String> = oath_core::diff_values(&desired, &applied)
            .into_iter()
            .map(|(k, _, _)| k)
            .filter(|k| k != "autologin")
            .collect();
        if !other.is_empty() {
            return Err(format!(
                "host:local has changes not applied ({}); apply or undo them first",
                other.join(", ")
            ));
        }
    }
    let oath = |args: &[&str]| -> Result<(), String> {
        let out = Command::new("/bin/oath")
            .arg("--via")
            .arg("svc:login")
            .args(args)
            .env_clear()
            .env("PATH", "/bin")
            .env("HOME", "/root")
            .output()
            .map_err(|e| format!("/bin/oath: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            let err = err.lines().filter(|l| !l.starts_with("oath-tel")).collect::<Vec<_>>();
            Err(format!("oath {}: {}", args.join(" "), err.join("; ")))
        }
    };
    oath(&["set", "host:local", &format!("autologin={on}")])?;
    oath(&["apply", "host:local"])?;
    eprintln!("login: autologin {}", if on { "on" } else { "off" });
    Ok(())
}

/// PID 1 converges (and so moves the seat) on any line to its socket.
fn notify_init() {
    match UnixStream::connect("/oath/run/init.sock") {
        Ok(mut s) => {
            let _ = s.write_all(b"seat\n");
            let mut ok = [0u8; 3];
            let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = s.read(&mut ok);
        }
        Err(e) => eprintln!("login: /oath/run/init.sock: {e}"),
    }
}

fn answer(out: &mut UnixStream, v: &Value) -> std::io::Result<()> {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("seat_changed");
    }
    let mut line = serde_json::to_vec(&v)?;
    line.push(b'\n');
    out.write_all(&line)
}

fn refuse(why: &str) -> Value {
    json!({ "ok": false, "error": why })
}

fn peer_uid(s: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ok = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    } == 0;
    ok.then_some(cred.uid)
}
