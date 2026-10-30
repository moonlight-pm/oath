//! The desk's password and its seat (R42).
//!
//! `svc:login` (`/lib/oath/login`, root) keeps `home`'s password, hashed
//! (Argon2id) in [`PASSWORD_FILE`], root's alone; none is set at first.
//! It answers `home` (and root) on [`SOCKET`], one JSON object a line,
//! each request answered by one line:
//!
//! - `{"call":"status"}` → `{"ok":true,"password":false,"autologin":true,"seat":"desk"}`
//! - `{"call":"check","password":"…"}` → `{"ok":true,"right":true}`; a
//!   wrong one answers `"right":false` after a delay that doubles with
//!   each wrong one in a row (1 s up to 30 s) and ends at a right one
//! - `{"call":"set","old":"…","new":"…"}` → `{"ok":true}`; an empty `new`
//!   removes the password, and `old` is needed only while one is set
//! - `{"call":"autologin","on":false,"password":"…"}` → `{"ok":true}`;
//!   `password` only while one is set. It sets `host:local.autologin`.
//! - `{"call":"desk","up":false}` → `{"ok":true}`: the desk ended on
//!   purpose, and the seat is the console's; `"up":true` gives it back
//!
//! A refusal is `{"ok":false,"error":"…"}`. The seat is [`SEAT_FILE`]
//! (`desk` or `console`; the desk's when absent, so every boot): PID 1
//! runs a service whose `seat` is the other one's held, and `svc:screen`
//! is the console's on a desk.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::Value;

use crate::error::{Error, Result};

/// The service's socket, `home`'s and root's (checked by SO_PEERCRED).
pub const SOCKET: &str = "/oath/run/login.sock";
/// Who has the screen and keyboard now: `desk` or `console`.
pub const SEAT_FILE: &str = "/oath/run/seat";
/// `home`'s password as an Argon2id PHC string; mode 0600, root's.
pub const PASSWORD_FILE: &str = "/oath/login/password";

/// The two owners of the seat (tty1, its screen and keyboard).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Desk,
    Console,
}

impl Seat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desk => "desk",
            Self::Console => "console",
        }
    }

    /// The seat now: the console's only when [`SEAT_FILE`] says so.
    pub fn current() -> Self {
        Self::read(std::path::Path::new(SEAT_FILE))
    }

    pub fn read(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) if s.trim() == "console" => Self::Console,
            _ => Self::Desk,
        }
    }
}

/// One request to `svc:login`, one answer. The answer's `ok: false` is
/// an `Err` with its `error`.
pub fn call(req: &Value) -> Result<Value> {
    let mut s = UnixStream::connect(SOCKET).map_err(|e| {
        Error::hint(format!("{SOCKET}: {e}"), "oath get svc:login (is svc:login running?)")
    })?;
    // A wrong password's answer waits up to 30 s.
    s.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut line = serde_json::to_vec(req)?;
    line.push(b'\n');
    s.write_all(&line)?;
    let mut answer = String::new();
    BufReader::new(&s).read_line(&mut answer)?;
    let v: Value = serde_json::from_str(answer.trim())
        .map_err(|e| Error::Msg(format!("svc:login answered {answer:?}: {e}")))?;
    if v.get("ok") == Some(&Value::Bool(true)) {
        Ok(v)
    } else {
        let why = v.get("error").and_then(Value::as_str).unwrap_or("refused");
        Err(Error::Msg(format!("svc:login: {why}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SvcSeat;

    #[test]
    fn the_seat_is_the_desks_unless_the_file_says_console() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("seat");
        assert_eq!(Seat::read(&f), Seat::Desk);
        std::fs::write(&f, "console\n").unwrap();
        assert_eq!(Seat::read(&f), Seat::Console);
        std::fs::write(&f, "desk\n").unwrap();
        assert_eq!(Seat::read(&f), Seat::Desk);
        std::fs::write(&f, "anything else").unwrap();
        assert_eq!(Seat::read(&f), Seat::Desk);
    }

    #[test]
    fn a_service_runs_in_its_owners_turn() {
        assert!(SvcSeat::Desk.runs_on(Seat::Desk));
        assert!(!SvcSeat::Desk.runs_on(Seat::Console));
        assert!(SvcSeat::Console.runs_on(Seat::Console));
        assert!(!SvcSeat::Console.runs_on(Seat::Desk));
        let svc: crate::Svc =
            serde_json::from_str(r#"{"exec": ["/bin/true"], "seat": "console"}"#).unwrap();
        assert_eq!(svc.seat, Some(SvcSeat::Console));
    }
}
