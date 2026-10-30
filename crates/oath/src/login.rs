//! `oath password` and `oath desk` (R42): `svc:login`'s clients for a
//! person at the console, an agent over SSH and a script.

use std::io::{BufRead, IsTerminal, Write};

use oath_core::login;
use oath_core::{Error, Result};
use serde_json::{json, Value};

/// Set, change or remove `home`'s password; `status` only says whether one
/// is set, with auto-login and the seat. From a terminal it asks (the
/// current password while one is set, then the new one twice); otherwise
/// it reads those lines from stdin. An empty new password removes it.
pub fn password(status: bool, as_json: bool) -> Result<i32> {
    let st = login::call(&json!({ "call": "status" }))?;
    let set = st.get("password") == Some(&json!(true));
    if status {
        if as_json {
            println!("{}", serde_json::to_string_pretty(&strip(st))?);
        } else {
            let on = |k: &str| st.get(k) == Some(&json!(true));
            println!(
                "password: {}\nautologin: {}\nseat: {}",
                if set { "set" } else { "none" },
                if on("autologin") { "on" } else { "off" },
                st.get("seat").and_then(Value::as_str).unwrap_or("desk"),
            );
        }
        return Ok(0);
    }
    let (old, new) = if std::io::stdin().is_terminal() {
        let old = if set { ask("Current password: ")? } else { String::new() };
        let new = ask("New password (empty removes it): ")?;
        if !new.is_empty() && ask("Again: ")? != new {
            return Err(Error::Msg("the two passwords differ; nothing changed".into()));
        }
        (old, new)
    } else {
        let mut lines = std::io::stdin().lock().lines();
        let mut next = || lines.next().transpose().map(Option::unwrap_or_default);
        let old = if set { next()? } else { String::new() };
        (old, next()?)
    };
    login::call(&json!({ "call": "set", "old": old, "new": new }))?;
    let done = if new.is_empty() { "removed" } else { "set" };
    if as_json {
        println!("{}", json!({ "password": done }));
    } else {
        println!("password {done}");
    }
    Ok(0)
}

/// Give the seat to the desk, or with `stop` end the desk on purpose and
/// give it to the console.
pub fn desk(stop: bool, as_json: bool) -> Result<i32> {
    let v = login::call(&json!({ "call": "desk", "up": !stop }))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&strip(v))?);
    } else if stop {
        println!("the console has the seat; `oath desk` gives it back to the desk");
    } else {
        println!("the desk has the seat");
    }
    Ok(0)
}

fn strip(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.remove("ok");
    }
    v
}

/// One line from the terminal, not echoed.
fn ask(prompt: &str) -> Result<String> {
    let mut err = std::io::stderr();
    let _ = write!(err, "{prompt}");
    let _ = err.flush();
    let fd = 0;
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    let quiet = unsafe { libc::tcgetattr(fd, &mut saved) } == 0;
    if quiet {
        let mut t = saved;
        t.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &t) };
    }
    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line);
    if quiet {
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    }
    let _ = writeln!(err);
    if n? == 0 {
        return Err(Error::Msg("no input; nothing changed".into()));
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
