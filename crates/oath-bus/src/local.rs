//! `/oath/run/bus.sock`: one JSON request a line, one answer a line, for
//! `oath consort` and a desk's view of the consort. Root and `home` may
//! read (SO_PEERCRED); only root, or the account `oath-bus` runs as (a
//! test's, a desk's own copy), changes who is in the consort.
//!
//! - `{"op":"state"}`: this machine, the members, machines nearby, asks.
//! - `{"op":"records","peer":"aulos","topic":"status"}`: facts (both optional).
//! - `{"op":"watch"}`: the state (`{"ok":true,"ev":"state",…}`), every
//!   record there is, `{"ev":"replayed"}`, then a line for each event:
//!   `state` (who is in it, online, nearby, asks) and `record` (a new fact).
//! - `{"op":"join","to":"<name or address>"}`, `{"op":"admit","name":…,
//!   "code":…}`, `{"op":"remove","name":…}`, `{"op":"leave"}`: root.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use serde_json::{json, Value};

use crate::{refuse, Bus};

pub fn start(bus: Arc<Bus>) -> std::io::Result<()> {
    let path = oath_core::consort::socket(&bus.root);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let _ = fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666))?;
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let b = bus.clone();
            thread::spawn(move || serve(b, conn));
        }
    });
    Ok(())
}

fn serve(bus: Arc<Bus>, s: UnixStream) {
    let uid = peer_uid(&s);
    let Ok(mut out) = s.try_clone() else { return };
    if !matches!(uid, Some(0) | Some(1)) {
        let _ = answer(&mut out, &refuse("only root and home may ask oath-bus", "run it as home, or with sudo"));
        return;
    }
    for line in BufReader::new(s).lines() {
        let Ok(line) = line else { return };
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = answer(&mut out, &refuse(&format!("not JSON: {e}"), "one JSON object a line"));
                continue;
            }
        };
        let op = req["op"].as_str().unwrap_or("");
        let root = uid == Some(0) || uid == Some(unsafe { libc::geteuid() });
        let reply = match op {
            "state" => {
                let st = bus.lock();
                let mut v = bus.state_json(&st);
                v["ok"] = json!(true);
                v
            }
            "records" => records(&bus, &req),
            "watch" => return watch(&bus, out),
            "join" | "admit" | "remove" | "leave" if !root => {
                refuse("changing the consort is root's", &format!("sudo oath consort {op}"))
            }
            "join" => bus.op_join(req["to"].as_str().unwrap_or("")),
            "admit" => bus.op_admit(req["name"].as_str().unwrap_or(""), req["code"].as_str().unwrap_or("")),
            "remove" => bus.op_remove(req["name"].as_str().unwrap_or("")),
            "leave" => bus.op_leave(),
            _ => refuse(&format!("no op {op:?}"), "state, records, watch, join, admit, remove, leave"),
        };
        if answer(&mut out, &reply).is_err() {
            return;
        }
    }
}

fn records(bus: &Bus, req: &Value) -> Value {
    let st = bus.lock();
    let members = st.log.members();
    let peer = req["peer"].as_str();
    let topic = req["topic"].as_str();
    let list: Vec<Value> = st
        .records
        .iter()
        .filter(|((by, t), _)| {
            topic.is_none_or(|w| w == t) && peer.is_none_or(|p| members.get(by).is_some_and(|m| m.name == p))
        })
        .map(|(_, (_, r))| bus.record_json(&st, r))
        .collect();
    json!({ "ok": true, "records": list })
}

/// The state, every record there is, `{"ev":"replayed"}`, then events as
/// they come. An event that lands while the replay is written comes after
/// it, perhaps twice: the newest `seq` per peer and topic wins.
fn watch(bus: &Bus, mut out: UnixStream) {
    let (tx, rx) = mpsc::channel::<String>();
    let mut lines = Vec::new();
    {
        let mut st = bus.lock();
        st.watchers.push(tx);
        lines.push(json!({ "ok": true, "ev": "state", "state": bus.state_json(&st) }));
        for (_, r) in st.records.values() {
            lines.push(json!({ "ev": "record", "record": bus.record_json(&st, r) }));
        }
        lines.push(json!({ "ev": "replayed" }));
    }
    for v in &lines {
        if answer(&mut out, v).is_err() {
            return;
        }
    }
    for line in rx {
        if out.write_all(format!("{line}\n").as_bytes()).is_err() {
            return;
        }
    }
}

fn answer(out: &mut UnixStream, v: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(v)?;
    line.push(b'\n');
    out.write_all(&line)
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
