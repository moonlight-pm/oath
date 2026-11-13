//! The facts a peer publishes about itself, each a topic: read from its own
//! catalog with `oath-core`, as `oath get` reads them, and from `/proc`.
//! Process names only, never command lines, which can hold secrets.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use oath_core::{Catalog, ObjectId, KIND_PKG, KIND_SVC};
use serde_json::{json, Value};

/// How many processes each list keeps.
const TOP: usize = 15;

pub struct Facts {
    /// CPU ticks per process when last read, for each one's share since.
    ticks: BTreeMap<i32, u64>,
    at: Option<Instant>,
}

impl Facts {
    pub fn new() -> Self {
        Self { ticks: BTreeMap::new(), at: None }
    }

    /// Every topic but `update`, which fetches from the origin and goes
    /// less often.
    pub fn gather(&mut self, root: &Path) -> Vec<(&'static str, Value)> {
        let cat = Catalog::open(root).ok();
        let mut out = Vec::new();
        if let Some(cat) = cat.as_ref().filter(|_| root.join("objects").is_dir()) {
            let services = services(cat);
            out.push(("status", status(root, cat, &services)));
            out.push(("services", Value::Array(services)));
            out.push(("packages", packages(root, cat)));
        }
        out.push(("load", load()));
        out.push(("processes", self.processes()));
        out
    }

    fn processes(&mut self) -> Value {
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64;
        let now = Instant::now();
        let secs = self.at.map(|a| now.duration_since(a).as_secs_f64()).unwrap_or(0.0);
        let mut ticks = BTreeMap::new();
        let mut procs = Vec::new();
        for e in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else { continue };
            let Ok(stat) = fs::read_to_string(e.path().join("stat")) else { continue };
            // `pid (comm) state …`: comm may hold spaces and parentheses.
            let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else { continue };
            let name = stat[open + 1..close].to_string();
            let f: Vec<&str> = stat[close + 1..].split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            // After comm: state is 0, utime 11, stime 12, rss 21.
            let t = num(11) + num(12);
            ticks.insert(pid, t);
            if num(21) == 0 {
                continue;
            }
            let cpu = match (self.ticks.get(&pid), secs > 0.0) {
                (Some(before), true) => (t.saturating_sub(*before)) as f64 / hz / secs * 100.0,
                _ => 0.0,
            };
            let uid = fs::read_to_string(e.path().join("status"))
                .ok()
                .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).map(String::from))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|u| u.parse::<u32>().ok()));
            procs.push((pid, name, uid, cpu, num(21) * page));
        }
        self.ticks = ticks;
        self.at = Some(now);
        let mut keep: Vec<usize> = Vec::new();
        let mut by = |key: &dyn Fn(&(i32, String, Option<u32>, f64, u64)) -> f64| {
            let mut idx: Vec<usize> = (0..procs.len()).collect();
            idx.sort_by(|a, b| key(&procs[*b]).total_cmp(&key(&procs[*a])));
            for i in idx.into_iter().take(TOP) {
                if !keep.contains(&i) {
                    keep.push(i);
                }
            }
        };
        by(&|p| p.3);
        by(&|p| p.4 as f64);
        let mut list: Vec<Value> = keep
            .into_iter()
            .map(|i| {
                let (pid, name, uid, cpu, rss) = &procs[i];
                json!({
                    "pid": pid,
                    "name": name,
                    "user": uid.map(user).unwrap_or_default(),
                    "cpu": (cpu * 10.0).round() / 10.0,
                    "rss": rss,
                })
            })
            .collect();
        list.sort_by(|a, b| b["cpu"].as_f64().unwrap_or(0.0).total_cmp(&a["cpu"].as_f64().unwrap_or(0.0)));
        Value::Array(list)
    }
}

/// `oath update`'s preview: what the newest output and heads would move.
pub fn update(root: &Path) -> Option<Value> {
    if root != Path::new("/oath") {
        return None;
    }
    let out = Command::new("/bin/oath").args(["--json", "update"]).output().ok()?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    if let Some(e) = v.get("error") {
        return Some(json!({ "error": e, "checked": oath_core::now_rfc3339() }));
    }
    let moves = v.get("moves").and_then(Value::as_array).cloned().unwrap_or_default();
    let names: Vec<Value> = moves
        .iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).cloned())
        .take(40)
        .collect();
    Some(json!({
        "output": v.get("output"),
        "runs": v.get("runs"),
        "moves": moves.len(),
        "moving": names,
        "refused": v.get("refused").and_then(Value::as_array).map(|r| r.len()).unwrap_or(0),
        "checked": oath_core::now_rfc3339(),
    }))
}

fn services(cat: &Catalog) -> Vec<Value> {
    let mut out = Vec::new();
    for id in cat.ls(Some(KIND_SVC)).unwrap_or_default() {
        let Ok(o) = cat.get(&id) else { continue };
        let a = &o.actual;
        out.push(json!({
            "name": id.name,
            "enabled": o.desired.get("enabled").cloned().unwrap_or(json!(false)),
            "state": a.get("state").cloned().unwrap_or(json!("")),
            "restarts": a.get("restarts").cloned().unwrap_or(json!(0)),
            "since": a.get("since").cloned().unwrap_or(Value::Null),
            // `{"code": n}` or `{"signal": n}`, as PID 1 writes it.
            "last_exit": a.get("last_exit").cloned().unwrap_or(Value::Null),
            "reason": a.get("reason").cloned().unwrap_or(Value::Null),
        }));
    }
    out
}

/// Each package's pin, whether it is an overlay (R47) as `oath get` says,
/// and the `source` it was built from when one is recorded.
/// PID 1 gave up on it, or it stopped with an error code or a signal. A
/// one-shot that exited 0 is done, not failed.
fn failed(s: &Value) -> bool {
    let bad_exit = s["last_exit"].get("signal").is_some() || s["last_exit"]["code"].as_i64().is_some_and(|c| c != 0);
    s["state"] == "failed" || (s["enabled"] == true && s["state"] == "stopped" && bad_exit)
}

fn packages(root: &Path, cat: &Catalog) -> Value {
    let mut out = Vec::new();
    for id in cat.ls(Some(KIND_PKG)).unwrap_or_default() {
        let Ok(o) = cat.get(&id) else { continue };
        let hash = o.actual.get("hash").or_else(|| o.desired.get("hash")).and_then(Value::as_str).unwrap_or("");
        let mut p = json!({
            "name": id.name,
            "hash": hash.trim_start_matches("sha256-").get(..12).unwrap_or(""),
            "present": o.desired.get("present").cloned().unwrap_or(json!(false)),
            "overlay": oath_core::update::overlay_of(root, &o).is_some(),
        });
        if let Some(src) = o.actual.get("source").filter(|s| !s.is_null()) {
            p["source"] = src.clone();
        }
        out.push(p);
    }
    Value::Array(out)
}

fn status(root: &Path, cat: &Catalog, services: &[Value]) -> Value {
    let host = cat.get(&ObjectId::new("host", "local")).map(|o| o.desired).unwrap_or_default();
    let output: Value = fs::read_to_string(root.join("output.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let generation = cat.get(&ObjectId::new("snap", "current")).ok().and_then(|o| o.desired.get("generation").cloned());
    let drift: Vec<String> = cat.diff(None).unwrap_or_default().iter().map(|d| d.id.to_string()).collect();
    let failed: Vec<&Value> = services.iter().filter(|s| failed(s)).map(|s| &s["name"]).collect();
    let running = services.iter().filter(|s| s["state"] == "running").count();
    json!({
        "profile": host.get("profile"),
        "output": output.get("output"),
        "published": output.get("published"),
        "generation": generation,
        "drift": drift.len(),
        "drifted": drift.iter().take(20).collect::<Vec<_>>(),
        "failed": failed,
        "services": { "running": running, "total": services.len() },
        "booted": booted(),
        "machine": machine(),
        "kernel": fs::read_to_string("/proc/sys/kernel/osrelease").map(|s| s.trim().to_string()).unwrap_or_default(),
    })
}

/// What the machine is: the firmware's maker and model, and the CPU.
fn machine() -> Value {
    let dmi = |f: &str| fs::read_to_string(format!("/sys/class/dmi/id/{f}")).map(|s| s.trim().to_string()).unwrap_or_default();
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default();
    json!({ "vendor": dmi("sys_vendor"), "product": dmi("product_name"), "cpu": cpu })
}

fn booted() -> Value {
    let btime = fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("btime ")).map(String::from))
        .and_then(|l| l[6..].trim().parse::<i64>().ok());
    match btime.and_then(oath_core::consort::rfc3339_of_secs) {
        Some(t) => json!(t),
        None => Value::Null,
    }
}

fn load() -> Value {
    let avg: Vec<f64> = fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    let mem: BTreeMap<String, u64> = fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.to_string(), v.split_whitespace().next()?.parse::<u64>().ok()? * 1024))
        })
        .collect();
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let disk = (unsafe { libc::statvfs(c"/".as_ptr(), &mut st) } == 0).then(|| {
        json!({ "size": st.f_blocks as u64 * st.f_frsize as u64, "free": st.f_bavail as u64 * st.f_frsize as u64 })
    });
    json!({
        "load": avg,
        "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        "memory": { "total": mem.get("MemTotal"), "available": mem.get("MemAvailable") },
        "disk": disk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_shot_that_finished_is_not_failed() {
        let s = |state: &str, exit: Value| json!({ "enabled": true, "state": state, "last_exit": exit });
        assert!(!failed(&s("stopped", json!({ "code": 0 }))));
        assert!(!failed(&s("scheduled", json!({ "code": 0 }))));
        assert!(!failed(&s("held", Value::Null)));
        assert!(failed(&s("stopped", json!({ "code": 1 }))));
        assert!(failed(&s("stopped", json!({ "signal": 9 }))));
        assert!(failed(&s("failed", Value::Null)));
    }
}

/// Oath's accounts by uid (R55).
fn user(uid: u32) -> String {
    match uid {
        0 => "root".into(),
        1 => "home".into(),
        2 => "work".into(),
        65534 => "void".into(),
        n => n.to_string(),
    }
}
