//! Surviving a panic. A panic in PID 1's main loop is caught; PID 1 writes
//! what it knows (the supervisor's state and the fd numbers of its log
//! pipes, `init.sock` and the uevent socket, all kept open) to
//! [`STATE`] and execs its own binary again with [`MARK`]. The services
//! never notice: they stay PID 1's children. The new image skips boot,
//! reads the state back and runs the main loop. A third panic within a
//! minute is not resumed: PID 1 shuts down cleanly and reboots instead.

use std::ffi::CString;
use std::fs;
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::net::UnixListener;

use oath_core::tel;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::log;
use crate::supervise::{self, Supervisor};

/// `argv[1]` of a resumed PID 1.
pub const MARK: &str = "oath-resume";
pub const STATE: &str = "/run/oath/init-resume.json";
/// Resumes allowed within [`WINDOW`] seconds before PID 1 reboots instead.
const LIMIT: usize = 2;
const WINDOW: f64 = 60.0;

#[derive(Serialize, Deserialize)]
pub struct Handoff {
    pub sup: supervise::State,
    pub listener: i32,
    pub uevents: Option<i32>,
    /// Seconds since boot of each resume so far.
    pub resumes: Vec<f64>,
}

/// What a resumed PID 1 continues with.
pub struct Resumed {
    pub sup: Supervisor,
    pub listener: UnixListener,
    pub uevents: Option<OwnedFd>,
    pub resumes: Vec<f64>,
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
    let keep = |fd: i32| unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
    let listener = listener.into_raw_fd();
    keep(listener);
    let uevents = uevents.map(|f| {
        let fd = f.into_raw_fd();
        keep(fd);
        fd
    });
    let state = Handoff { sup: sup.export(), listener, uevents, resumes };
    let running = state.sup.len();
    if let Err(e) = oath_core::write_json(std::path::Path::new(STATE), &state) {
        log(&format!("panic: cannot write {STATE}: {e}; rebooting"));
        return;
    }
    log(&format!("panic: re-executing PID 1; {running} service processes keep running"));
    tel("init", "resume", json!({ "ok": true, "running": running }));
    let exe = CString::new("/proc/self/exe").unwrap();
    let argv = [CString::new("/init").unwrap(), CString::new(MARK).unwrap()];
    let argv: Vec<*const libc::c_char> =
        argv.iter().map(|a| a.as_ptr()).chain(std::iter::once(std::ptr::null())).collect();
    unsafe { libc::execv(exe.as_ptr(), argv.as_ptr()) };
    log(&format!("panic: exec failed: {}; rebooting", std::io::Error::last_os_error()));
}

/// Whether this process is a resumed PID 1.
pub fn resuming() -> bool {
    std::process::id() == 1 && std::env::args().nth(1).as_deref() == Some(MARK)
}

/// Read the handoff back and take over its fds.
pub fn take() -> Result<Resumed, String> {
    let text = fs::read_to_string(STATE).map_err(|e| format!("{STATE}: {e}"))?;
    let _ = fs::remove_file(STATE);
    let h: Handoff = serde_json::from_str(&text).map_err(|e| format!("{STATE}: {e}"))?;
    let own = |fd: i32| {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        fd
    };
    let listener = unsafe { UnixListener::from_raw_fd(own(h.listener)) };
    let _ = listener.set_nonblocking(true);
    let uevents = h.uevents.map(|fd| unsafe { OwnedFd::from_raw_fd(own(fd)) });
    let sup = Supervisor::import(h.sup);
    log(&format!(
        "resumed after a panic ({} in the last minute); {} service processes kept",
        h.resumes.len(),
        sup.running_count()
    ));
    Ok(Resumed { sup, listener, uevents, resumes: h.resumes })
}
