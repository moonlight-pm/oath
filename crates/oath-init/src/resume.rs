//! Re-executing PID 1. A panic in PID 1's main loop is caught; PID 1 writes
//! what it knows (the supervisor's state and the fd numbers of its log
//! pipes, `init.sock` and the uevent socket, all kept open) to
//! [`STATE`] and execs its own binary again with [`MARK`]. The services
//! never notice: they stay PID 1's children. The new image skips boot,
//! reads the state back and runs the main loop. A third panic within a
//! minute is not resumed: PID 1 shuts down cleanly and reboots instead.
//!
//! `oath update` asks the same of a new binary (R73): `upgrade <path>
//! <hash>` on `init.sock`, from root. PID 1 opens the file once, hashes
//! what it opened, has that binary read a copy of the state first
//! ([`REHEARSE`]), and only then hands over and execs the very file it
//! hashed. A new image that cannot take the state execs the one that handed
//! over (`previous=<fd>`), which can. Boot-only work (mounts, modules,
//! sysctls, device modes, `/etc`) does not run again: it waits for a boot.

use std::ffi::CString;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use oath_core::tel;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::hotplug::Hotplug;
use crate::log;
use crate::supervise::{self, Supervisor};

/// `argv[1]` of a resumed PID 1.
pub const MARK: &str = "oath-resume";
/// `argv[1]` of a binary PID 1 tries before it re-executes into it: it reads
/// the state at `argv[2]` and says so (R73).
pub const REHEARSE: &str = "oath-rehearse";
pub const STATE: &str = "/run/oath/init-resume.json";
/// The copy of the state a new binary rehearses with.
const REHEARSAL: &str = "/run/oath/init-rehearse.json";
/// What PID 1 says of itself, written at boot and by each image that takes
/// over (R73): that it takes `upgrade` (`/run` is new each boot, so an older
/// PID 1 never leaves one), its binary's hash, and how it started. `oath
/// update` asks only a PID 1 that wrote it, since an older one takes any
/// other message for a converge.
pub const SELF: &str = "/run/oath/init.json";
/// The handoff's format. A reader refuses a newer one.
pub const FORMAT: u32 = 1;
/// Resumes allowed within [`WINDOW`] seconds before PID 1 reboots instead.
const LIMIT: usize = 2;
const WINDOW: f64 = 60.0;
/// How long a new binary may take to read the state.
const REHEARSE_WAIT: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize)]
pub struct Handoff {
    /// [`FORMAT`]. A PID 1 from before R73 wrote none: format 1.
    #[serde(default = "first_format")]
    pub format: u32,
    pub sup: supervise::State,
    pub listener: i32,
    pub uevents: Option<i32>,
    /// Seconds since boot of each resume after a panic so far.
    pub resumes: Vec<f64>,
    /// `panic`, or `upgrade` (R73).
    #[serde(default = "panic_word")]
    pub why: String,
}

fn first_format() -> u32 {
    1
}

fn panic_word() -> String {
    "panic".into()
}

impl Handoff {
    /// Every fd it hands over.
    fn fds(&self) -> Vec<i32> {
        let mut fds = self.sup.fds();
        fds.push(self.listener);
        fds.extend(self.uevents);
        fds
    }

    /// Let its fds through the exec (`true`), or close them on exec again.
    fn inherit(&self, through: bool) {
        for fd in self.fds() {
            let flag = if through { 0 } else { libc::FD_CLOEXEC };
            unsafe { libc::fcntl(fd, libc::F_SETFD, flag) };
        }
    }
}

/// Read a handoff, refusing a format newer than this binary knows.
pub fn parse(text: &str) -> Result<Handoff, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let format = v.get("format").and_then(Value::as_u64).unwrap_or(1);
    if format > u64::from(FORMAT) {
        return Err(format!("the handoff is format {format}; this PID 1 reads up to {FORMAT}"));
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// What a resumed PID 1 continues with.
pub struct Resumed {
    pub sup: Supervisor,
    pub listener: UnixListener,
    pub uevents: Option<OwnedFd>,
    pub resumes: Vec<f64>,
    pub why: String,
}

/// After a caught panic: exec again with the state, or, after too many,
/// say so and return (the caller reboots).
pub fn handoff(
    sup: &mut Supervisor,
    listener: UnixListener,
    uevents: Option<OwnedFd>,
    mut resumes: Vec<f64>,
) {
    let now = oath_core::probation::uptime();
    resumes.retain(|t| now - t < WINDOW);
    if resumes.len() >= LIMIT {
        log(&format!(
            "panic: {} times in {WINDOW} s; not resuming again, rebooting",
            resumes.len() + 1
        ));
        tel("init", "resume", json!({ "ok": false, "recent": resumes.len() }));
        return;
    }
    resumes.push(now);
    let state = Handoff {
        format: FORMAT,
        sup: sup.export(),
        listener: listener.into_raw_fd(),
        uevents: uevents.map(IntoRawFd::into_raw_fd),
        resumes,
        why: panic_word(),
    };
    state.inherit(true);
    let running = state.sup.len();
    if let Err(e) = oath_core::write_json(Path::new(STATE), &state) {
        log(&format!("panic: cannot write {STATE}: {e}; rebooting"));
        return;
    }
    log(&format!("panic: re-executing PID 1; {running} service processes keep running"));
    tel("init", "resume", json!({ "ok": true, "running": running }));
    let exe = CString::new("/proc/self/exe").unwrap();
    let argv = argv(None);
    let ptrs = ptrs(&argv);
    unsafe { libc::execv(exe.as_ptr(), ptrs.as_ptr()) };
    log(&format!("panic: exec failed: {}; rebooting", std::io::Error::last_os_error()));
}

/// `/init oath-resume [previous=<fd>]`.
fn argv(previous: Option<i32>) -> Vec<CString> {
    let mut v = vec![CString::new("/init").unwrap(), CString::new(MARK).unwrap()];
    if let Some(fd) = previous {
        v.push(CString::new(format!("previous={fd}")).unwrap());
    }
    v
}

fn ptrs(argv: &[CString]) -> Vec<*const libc::c_char> {
    argv.iter().map(|a| a.as_ptr()).chain(std::iter::once(std::ptr::null())).collect()
}

/// Whether this process is a resumed PID 1.
pub fn resuming() -> bool {
    std::process::id() == 1 && std::env::args().nth(1).as_deref() == Some(MARK)
}

/// The image that handed over by `upgrade`, when this one is its successor.
fn previous() -> Option<i32> {
    std::env::args().skip(2).find_map(|a| a.strip_prefix("previous=")?.parse().ok())
}

/// Read the handoff back and take over its fds.
pub fn take() -> Result<Resumed, String> {
    let text = fs::read_to_string(STATE).map_err(|e| format!("{STATE}: {e}"))?;
    let h = parse(&text).map_err(|e| format!("{STATE}: {e}"))?;
    // Only now: an image that cannot read it leaves it for the one before.
    let _ = fs::remove_file(STATE);
    if let Some(fd) = previous() {
        unsafe { libc::close(fd) };
    }
    let own = |fd: i32| {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        fd
    };
    let listener = unsafe { UnixListener::from_raw_fd(own(h.listener)) };
    let _ = listener.set_nonblocking(true);
    let uevents = h.uevents.map(|fd| unsafe { OwnedFd::from_raw_fd(own(fd)) });
    let sup = Supervisor::import(h.sup);
    let running = sup.running_count();
    if h.why == "upgrade" {
        log(&format!("upgrade: this binary took over; {running} service processes kept"));
        tel("init", "upgrade", json!({ "ok": true, "running": running }));
    } else {
        log(&format!(
            "resumed after a panic ({} in the last minute); {running} service processes kept",
            h.resumes.len(),
        ));
    }
    Ok(Resumed { sup, listener, uevents, resumes: h.resumes, why: h.why })
}

/// A new image could not take the state (`why`): exec the one that handed
/// over, which wrote it, with the state left where it was. Returns only
/// when there is none or that exec fails.
pub fn back(why: &str) {
    let Some(fd) = previous() else { return };
    log(&format!("upgrade: this binary cannot take over ({why}); executing the one before"));
    tel("init", "upgrade", json!({ "ok": false, "back": true, "why": why }));
    let argv = argv(None);
    let ptrs = ptrs(&argv);
    unsafe { libc::fexecve(fd, ptrs.as_ptr(), environ()) };
    log(&format!("upgrade: cannot execute the one before: {}", std::io::Error::last_os_error()));
}

fn environ() -> *const *const libc::c_char {
    extern "C" {
        static environ: *const *const libc::c_char;
    }
    unsafe { environ }
}

/// Say what this PID 1 is ([`SELF`]): `how` it started (`boot`, `panic`,
/// `upgrade`).
pub fn announce(how: &str) {
    let exe = fs::read("/proc/self/exe").map(|b| oath_core::file_hash(&b)).unwrap_or_default();
    log(&format!("PID 1 is {exe} ({how})"));
    let me = json!({
        "upgrade": 1,
        "format": FORMAT,
        "exe": exe,
        "how": how,
        "since": oath_core::now_rfc3339(),
    });
    if let Err(e) = oath_core::write_json(Path::new(SELF), &me) {
        log(&format!("cannot write {SELF}: {e}"));
    }
}

/// `upgrade <path> <hash>` (R73), from root.
pub struct Upgrade {
    pub path: PathBuf,
    pub hash: String,
    pub reply: UnixStream,
}

/// The binary and hash of an `upgrade` message.
pub fn parse_request(msg: &str) -> Result<(PathBuf, String), String> {
    let mut w = msg.split_whitespace();
    match (w.next(), w.next(), w.next(), w.next()) {
        (Some("upgrade"), Some(p), Some(h), None) => {
            if !p.starts_with('/') {
                return Err(format!("{p} is not an absolute path"));
            }
            if !oath_core::is_realization_id(h) {
                return Err(format!("{h} is not sha256-<64 hex>"));
            }
            Ok((PathBuf::from(p), h.to_string()))
        }
        _ => Err("upgrade <absolute path> sha256-<64 hex>".into()),
    }
}

/// Re-execute into the binary `req` names. Returns only when it did not:
/// the requester has been told why, and the supervisor, its sockets and
/// every service are as they were.
pub fn upgrade(sup: &mut Supervisor, listener: &UnixListener, hot: &mut Hotplug, resumes: &[f64], req: Upgrade) {
    let Upgrade { path, hash, mut reply } = req;
    let why = match try_upgrade(sup, listener, hot, resumes, &path, &hash, &mut reply) {
        Ok(()) => return,
        Err(why) => why,
    };
    log(&format!("upgrade: not into {}: {why}", path.display()));
    tel("init", "upgrade", json!({ "ok": false, "why": why }));
    let _ = reply.write_all(format!("refused: {why}\n").as_bytes());
}

fn try_upgrade(
    sup: &mut Supervisor,
    listener: &UnixListener,
    hot: &mut Hotplug,
    resumes: &[f64],
    path: &Path,
    hash: &str,
    reply: &mut UnixStream,
) -> Result<(), String> {
    // One open file, hashed and then executed: what runs is what was hashed.
    let mut bin = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !bin.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let mut bytes = Vec::new();
    bin.read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let got = oath_core::file_hash(&bytes);
    if got != hash {
        return Err(format!("{} hashes to {got}, not {hash}", path.display()));
    }
    drop(bytes);
    log(&format!("upgrade: {} ({hash}): rehearsing", path.display()));
    let uevents = hot.take_socket();
    let state = Handoff {
        format: FORMAT,
        sup: sup.export(),
        listener: listener.as_raw_fd(),
        uevents: uevents.as_ref().map(AsRawFd::as_raw_fd),
        resumes: resumes.to_vec(),
        why: "upgrade".into(),
    };
    let restore = |sup: &mut Supervisor, hot: &mut Hotplug, state: Handoff, uevents| {
        state.inherit(false);
        *sup = Supervisor::import(state.sup);
        hot.restore_socket(uevents);
    };
    if let Err(e) = rehearse(&bin, &state) {
        restore(sup, hot, state, uevents);
        return Err(format!("its rehearsal: {e}"));
    }
    // The image to go back to, open across the exec.
    let exe = CString::new("/proc/self/exe").unwrap();
    let previous = unsafe { libc::open(exe.as_ptr(), libc::O_RDONLY) };
    state.inherit(true);
    if let Err(e) = oath_core::write_json(Path::new(STATE), &state) {
        if previous >= 0 {
            unsafe { libc::close(previous) };
        }
        restore(sup, hot, state, uevents);
        return Err(format!("cannot write {STATE}: {e}"));
    }
    let running = state.sup.len();
    log(&format!(
        "upgrade: re-executing PID 1 into {} ({hash}); {running} service processes keep running",
        path.display()
    ));
    tel("init", "upgrade", json!({ "ok": true, "running": running, "hash": hash }));
    let _ = reply.write_all(b"upgrading\n");
    let argv = argv((previous >= 0).then_some(previous));
    let ptrs = ptrs(&argv);
    unsafe { libc::fexecve(bin.as_raw_fd(), ptrs.as_ptr(), environ()) };
    let e = std::io::Error::last_os_error();
    let _ = fs::remove_file(STATE);
    if previous >= 0 {
        unsafe { libc::close(previous) };
    }
    restore(sup, hot, state, uevents);
    Err(format!("exec: {e}"))
}

/// Have the binary `bin` read a copy of `state` before PID 1 becomes it: it
/// must print [`rehearsed`] for that copy, which only a PID 1 that read it
/// can.
fn rehearse(bin: &fs::File, state: &Handoff) -> Result<(), String> {
    let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
    oath_core::write_atomic(Path::new(REHEARSAL), text.as_bytes())
        .map_err(|e| format!("cannot write {REHEARSAL}: {e}"))?;
    let result = run_rehearsal(bin, &rehearsed(&text, state.sup.len()));
    let _ = fs::remove_file(REHEARSAL);
    result
}

fn run_rehearsal(bin: &fs::File, want: &str) -> Result<(), String> {
    let mut child = Command::new(format!("/proc/self/fd/{}", bin.as_raw_fd()))
        .arg0("/init")
        .args([REHEARSE, REHEARSAL])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start it: {e}"))?;
    let deadline = Instant::now() + REHEARSE_WAIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            other => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match other {
                    Err(e) => format!("waiting for it: {e}"),
                    _ => format!("it did not finish within {} s", REHEARSE_WAIT.as_secs()),
                });
            }
        }
    };
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if status.success() && out.lines().any(|l| l == want) {
        return Ok(());
    }
    let said = err.trim().lines().last().unwrap_or("").to_string();
    Err(match (status.success(), said.is_empty()) {
        (true, _) => "it did not say it read the state (not an Oath PID 1?)".to_string(),
        (false, true) => format!("it ended with {status}"),
        (false, false) => format!("it ended with {status}: {said}"),
    })
}

/// What a binary that read the state `text` (with `services` service
/// processes) prints.
pub fn rehearsed(text: &str, services: usize) -> String {
    format!("{REHEARSE} ok {} {services} services", oath_core::file_hash(text.as_bytes()))
}

/// `/init oath-rehearse <state>`, run by the PID 1 this binary may replace.
pub fn rehearse_main(path: Option<&str>) -> i32 {
    let Some(path) = path else {
        eprintln!("{REHEARSE}: no state file");
        return 2;
    };
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return 1;
        }
    };
    match parse(&text) {
        Ok(h) => {
            println!("{}", rehearsed(&text, h.sup.len()));
            0
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn handoff(format: u32) -> Handoff {
        Handoff {
            format,
            sup: supervise::State::default(),
            listener: 3,
            uevents: None,
            resumes: vec![],
            why: "upgrade".into(),
        }
    }

    #[test]
    fn a_handoff_reads_back() {
        let text = serde_json::to_string(&handoff(FORMAT)).unwrap();
        let h = parse(&text).unwrap();
        assert_eq!((h.format, h.listener, h.why.as_str()), (FORMAT, 3, "upgrade"));
    }

    #[test]
    fn a_newer_format_is_refused() {
        let text = serde_json::to_string(&handoff(FORMAT + 1)).unwrap();
        let e = parse(&text).err().unwrap();
        assert!(e.contains(&format!("format {}", FORMAT + 1)), "{e}");
    }

    #[test]
    fn a_handoff_from_before_r73_is_a_panics_format_1() {
        // What a PID 1 from before R73 wrote: no format, no why.
        let mut v = serde_json::to_value(handoff(FORMAT)).unwrap();
        v.as_object_mut().unwrap().remove("format");
        v.as_object_mut().unwrap().remove("why");
        let h = parse(&v.to_string()).unwrap();
        assert_eq!((h.format, h.why.as_str()), (1, "panic"));
    }

    #[test]
    fn requests_parse() {
        let (p, h) = parse_request(&format!("upgrade /oath/run/update/init {H}\n")).unwrap();
        assert_eq!((p, h.as_str()), (PathBuf::from("/oath/run/update/init"), H));
        for bad in [
            format!("upgrade init {H}"),
            "upgrade /init sha256-zz".to_string(),
            "upgrade /init".to_string(),
            format!("upgrade /init {H} more"),
            format!("converge {H}"),
        ] {
            assert!(parse_request(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_reader_of_the_state_rehearses() {
        let text = serde_json::to_string(&handoff(FORMAT)).unwrap();
        let said = rehearsed(&text, 0);
        assert!(said.starts_with("oath-rehearse ok sha256-"));
        // `echo` given the same arguments says something else.
        assert_ne!(said, format!("{REHEARSE} {REHEARSAL}"));
        assert_ne!(said, rehearsed(&format!("{text} "), 0));
    }
}
