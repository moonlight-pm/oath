//! PID 1's supervisor. It runs what was applied (`applied.json`) in
//! `wants` order, gives every service its own cgroup and log, restarts
//! with backoff, and gives up into `failed` after repeated quick crashes.
//! It also runs the DHCP client for `net:net0` when that is `dhcp`.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use oath_core::{now_rfc3339, tel, Catalog, Host, ObjectId, Svc, SvcRestart, DEFAULT_ROOT};
use serde_json::{json, Value};

use crate::{applied, log};

/// Service logs: `/oath/log/<kind>/<name>.log`, plus `.1` after rotation.
pub const LOG_ROOT: &str = "/oath/log";
const LOG_MAX: u64 = 256 * 1024;
const CGROUP_ROOT: &str = "/sys/fs/cgroup/oath";
/// A run shorter than this is a quick failure.
const QUICK: Duration = Duration::from_secs(10);
/// After this many quick failures in a row the service is `failed`.
const GIVE_UP: u32 = 5;
const STOP_WAIT: Duration = Duration::from_secs(5);
/// The DHCP client for `net:net0`. Not a catalog `svc`; its log is
/// `/oath/log/net/net0.log`.
const DHCP_ID: &str = "net:net0";

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
enum Exit {
    Code(i32),
    Signal(i32),
}

impl Exit {
    fn json(self) -> Value {
        match self {
            Self::Code(c) => json!({ "code": c }),
            Self::Signal(s) => json!({ "signal": s }),
        }
    }

    fn failed(self) -> bool {
        !matches!(self, Self::Code(0))
    }
}

/// The package realization a service's program runs from, `(name,
/// hash)`: `exec[0]` followed through `/bin` into the store.
type Program = Option<(String, String)>;

struct Kid {
    id: String,
    spec: Svc,
    /// What `exec[0]` resolved to when it started.
    program: Program,
    started: Instant,
    /// Read end of the child's stdout+stderr pipe (non-blocking).
    out: Option<fs::File>,
    partial: Vec<u8>,
}

#[derive(Default)]
struct History {
    restarts: u32,
    quick: u32,
    last_exit: Option<Exit>,
    /// Why it is `failed`, for actual.
    why: Option<String>,
}

struct Pending {
    id: String,
    spec: Svc,
    due: Instant,
}

#[derive(Default)]
pub struct Supervisor {
    kids: HashMap<i32, Kid>,
    oneshot_done: HashSet<String>,
    pending: Vec<Pending>,
    /// Gave up on this spec and program; a different applied spec, or the
    /// program moving to another realization, tries again.
    failed: HashMap<String, (Svc, Program)>,
    history: HashMap<String, History>,
    /// One-shot helpers PID 1 runs for itself (the probation rollback), by
    /// pid. Their output goes to `init.log`.
    tasks: HashMap<i32, String>,
    /// How each helper last ended, by name, until [`Supervisor::task_exit`]
    /// takes it.
    task_exits: HashMap<String, bool>,
}

/// Everything the supervisor knows, for a PID 1 that execs itself after a
/// panic (`crate::resume`). Children stay PID 1's across the exec; their
/// log pipes stay open (the fd numbers are here).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    kids: Vec<KidState>,
    oneshot_done: Vec<String>,
    pending: Vec<(String, Svc, u64)>,
    failed: Vec<(String, Svc, Program)>,
    history: Vec<(String, u32, u32, Option<Exit>, Option<String>)>,
    tasks: Vec<(i32, String)>,
}

impl State {
    /// Service processes in it.
    pub fn len(&self) -> usize {
        self.kids.len()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct KidState {
    pid: i32,
    id: String,
    spec: Svc,
    #[serde(default)]
    program: Program,
    /// Milliseconds it had run.
    ran_ms: u64,
    out: Option<i32>,
    partial: Vec<u8>,
}

impl Supervisor {
    /// Hand everything over: the log pipes lose close-on-exec so the next
    /// PID 1 can read them.
    pub fn export(&mut self) -> State {
        let now = Instant::now();
        let kids = self
            .kids
            .drain()
            .map(|(pid, k)| {
                let out = k.out.map(|f| {
                    let fd = std::os::fd::IntoRawFd::into_raw_fd(f);
                    unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
                    fd
                });
                KidState {
                    pid,
                    id: k.id,
                    spec: k.spec,
                    program: k.program,
                    ran_ms: k.started.elapsed().as_millis() as u64,
                    out,
                    partial: k.partial,
                }
            })
            .collect();
        State {
            kids,
            oneshot_done: self.oneshot_done.drain().collect(),
            pending: self
                .pending
                .drain(..)
                .map(|p| (p.id, p.spec, p.due.saturating_duration_since(now).as_millis() as u64))
                .collect(),
            failed: self.failed.drain().map(|(id, (spec, prog))| (id, spec, prog)).collect(),
            history: self
                .history
                .drain()
                .map(|(id, h)| (id, h.restarts, h.quick, h.last_exit, h.why))
                .collect(),
            tasks: self.tasks.drain().collect(),
        }
    }

    /// The supervisor a resumed PID 1 continues with.
    pub fn import(state: State) -> Self {
        let now = Instant::now();
        let mut s = Self::new();
        for k in state.kids {
            let out = k.out.map(|fd| {
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
                unsafe { fs::File::from_raw_fd(fd) }
            });
            let started = now.checked_sub(Duration::from_millis(k.ran_ms)).unwrap_or(now);
            s.kids.insert(
                k.pid,
                Kid {
                    id: k.id,
                    spec: k.spec,
                    program: k.program,
                    started,
                    out,
                    partial: k.partial,
                },
            );
        }
        s.oneshot_done = state.oneshot_done.into_iter().collect();
        s.pending = state
            .pending
            .into_iter()
            .map(|(id, spec, ms)| Pending { id, spec, due: now + Duration::from_millis(ms) })
            .collect();
        s.failed = state.failed.into_iter().map(|(id, spec, prog)| (id, (spec, prog))).collect();
        s.history = state
            .history
            .into_iter()
            .map(|(id, restarts, quick, last_exit, why)| {
                (id, History { restarts, quick, last_exit, why })
            })
            .collect();
        s.tasks = state.tasks.into_iter().collect();
        s
    }

    pub fn running_count(&self) -> usize {
        self.kids.len()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        let _ = fs::create_dir_all(CGROUP_ROOT);
        Self::default()
    }

    /// Make the running set match what is applied.
    pub fn converge(&mut self) {
        let (mut wanted, unreadable) = wanted();
        // A spec PID 1 cannot read leaves that service as it is.
        for id in unreadable {
            if let Some(k) = self.kids.values().find(|k| k.id == id) {
                wanted.insert(id, k.spec.clone());
            }
        }

        let running: Vec<(String, Svc, Program)> =
            self.kids.values().map(|k| (k.id.clone(), k.spec.clone(), k.program.clone())).collect();
        for (id, spec, was) in running {
            let Some(want) =
                wanted.get(&id).filter(|s| s.enabled && !s.exec.is_empty() && s.same_spawn(&spec))
            else {
                self.stop(&id);
                continue;
            };
            // A service follows its package: an apply that moved the
            // realization its program resolves to restarts it.
            if let (Some((name, hash)), true) = (program(want), id.starts_with("svc:")) {
                if was.as_ref().is_some_and(|w| *w != (name.clone(), hash.clone())) {
                    log(&format!("{id}: {} is now pkg:{name} {hash}; restarting", want.exec[0]));
                    tel("init", "svc_follow", json!({ "id": id, "pkg": name, "hash": hash }));
                    self.stop(&id);
                }
            }
        }
        self.pending
            .retain(|p| wanted.get(&p.id).is_some_and(|s| s.enabled && s.same_spawn(&p.spec)));
        // A new spec, or a new program, gets a fresh run of quick failures.
        let retry: Vec<String> = self
            .failed
            .iter()
            .filter(|(id, (spec, prog))| {
                !wanted.get(*id).is_some_and(|s| s == spec && program(s) == *prog)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in retry {
            self.failed.remove(&id);
            if let Some(h) = self.history.get_mut(&id) {
                h.quick = 0;
            }
        }

        let items: Vec<(String, Svc)> =
            wanted.iter().map(|(id, s)| (id.clone(), s.clone())).collect();
        let (order, stuck) = oath_core::svc_start_order_lenient(&items);
        for (id, why) in oath_core::svc_stuck_reasons(&items, &stuck) {
            log(&format!("{id} not started: {why}"));
            tel("init", "svc_wants", json!({ "ok": false, "id": id, "err": why }));
            if !self.running(&id) {
                self.write_actual_why(&id, "failed", None, Some(&why));
            }
        }
        for id in &order {
            let Some(spec) = wanted.get(id) else { continue };
            if spec.exec.is_empty() || self.running(id) || self.pending.iter().any(|p| &p.id == id)
            {
                continue;
            }
            if self.failed.contains_key(id) {
                let why = self.history.get(id.as_str()).and_then(|h| h.why.clone());
                self.write_actual_why(id, "failed", None, why.as_deref());
                continue;
            }
            if spec.restart == SvcRestart::Never && self.oneshot_done.contains(id) {
                continue;
            }
            self.start(id, spec);
        }
        for (id, spec) in &wanted {
            if spec.enabled {
                continue;
            }
            self.oneshot_done.remove(id);
            if !self.running(id) {
                self.write_actual(id, "stopped", None);
            }
        }
    }

    /// Collect exited children, record how they ended, schedule restarts.
    pub fn reap(&mut self) {
        loop {
            let (pid, exit) = match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(pid, code)) => (pid, Exit::Code(code)),
                Ok(WaitStatus::Signaled(pid, sig, _)) => (pid, Exit::Signal(sig as i32)),
                Ok(WaitStatus::StillAlive) | Err(_) => break,
                Ok(_) => continue,
            };
            if let Some(name) = self.tasks.remove(&pid.as_raw()) {
                log(&format!("{name}: exit {exit:?}"));
                self.task_exits.insert(name, !exit.failed());
                continue;
            }
            let Some(mut kid) = self.kids.remove(&pid.as_raw()) else { continue };
            pump(&mut kid);
            let ran = kid.started.elapsed();
            let h = self.history.entry(kid.id.clone()).or_default();
            h.last_exit = Some(exit);
            h.quick = if ran < QUICK { h.quick + 1 } else { 0 };
            let (quick, restarts) = (h.quick, h.restarts);
            tel(
                "init",
                "svc_exit",
                json!({ "id": kid.id, "exit": exit.json(), "ran_ms": ran.as_millis() as u64 }),
            );
            let again = match kid.spec.restart {
                SvcRestart::Never => false,
                SvcRestart::Always => true,
                SvcRestart::OnFailure => exit.failed(),
            };
            if kid.spec.restart == SvcRestart::Never {
                self.oneshot_done.insert(kid.id.clone());
            }
            if !kid.spec.keep_children {
                kill_cgroup(&kid.id);
            }
            if !again {
                self.write_actual(&kid.id, "stopped", None);
            } else if quick >= GIVE_UP {
                log(&format!(
                    "{}: {quick} quick failures in a row; giving up until the spec or the \
                     program changes",
                    kid.id
                ));
                let why = format!(
                    "exited within {} s {quick} times in a row; waits for a new spec or program",
                    QUICK.as_secs()
                );
                self.history.entry(kid.id.clone()).or_default().why = Some(why.clone());
                self.failed.insert(kid.id.clone(), (kid.spec.clone(), kid.program.clone()));
                self.write_actual_why(&kid.id, "failed", None, Some(&why));
            } else {
                // 1 s, 2 s, 4 s, 8 s after quick failures; at once after a long run.
                let delay = if quick == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1 << (quick - 1).min(5))
                };
                log(&format!(
                    "{}: exit {:?}, restart #{} in {}s",
                    kid.id,
                    exit,
                    restarts + 1,
                    delay.as_secs()
                ));
                self.pending.push(Pending {
                    id: kid.id.clone(),
                    spec: kid.spec,
                    due: Instant::now() + delay,
                });
                self.write_actual(&kid.id, "restarting", None);
            }
        }
    }

    /// Run a one-shot helper as root with PID 1's environment, its output
    /// appended to `init.log`. `reap` collects it.
    pub fn run_task(&mut self, name: &str, argv: &[&str]) {
        let mut cmd = Command::new(argv[0]);
        cmd.args(&argv[1..]).env_clear().env("PATH", "/bin").env("HOME", "/root");
        let out = fs::OpenOptions::new().create(true).append(true).open(log_path("init"));
        match out.and_then(|f| Ok((f.try_clone()?, f))) {
            Ok((o, e)) => cmd.stdin(Stdio::null()).stdout(o).stderr(e),
            Err(_) => cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()),
        };
        match cmd.spawn() {
            Ok(child) => {
                self.tasks.insert(child.id() as i32, name.to_string());
            }
            Err(e) => {
                log(&format!("{name}: cannot start {}: {e}", argv[0]));
                self.task_exits.insert(name.to_string(), false);
            }
        }
    }

    pub fn task_running(&self, name: &str) -> bool {
        self.tasks.values().any(|n| n == name)
    }

    /// Whether helper `name` succeeded, once, after it has ended.
    pub fn task_exit(&mut self, name: &str) -> Option<bool> {
        self.task_exits.remove(name)
    }

    /// Start restarts whose backoff is over.
    pub fn start_due(&mut self) {
        let now = Instant::now();
        let due: Vec<Pending> = {
            let (due, wait): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|p| p.due <= now);
            self.pending = wait;
            due
        };
        for p in due {
            if !self.running(&p.id) {
                self.history.entry(p.id.clone()).or_default().restarts += 1;
                self.start(&p.id, &p.spec);
            }
        }
    }

    /// Move child output into the log files.
    pub fn pump_logs(&mut self) {
        for kid in self.kids.values_mut() {
            pump(kid);
        }
    }

    /// Stop everything for shutdown, in reverse `wants` order: a service
    /// stops before the ones it wants. Each layer gets TERM, up to 5 s,
    /// then KILL.
    pub fn stop_all(&mut self) {
        let running: Vec<(String, Svc)> =
            self.kids.values().map(|k| (k.id.clone(), k.spec.clone())).collect();
        for layer in oath_core::svc_stop_layers(&running) {
            log(&format!("shutdown: stopping {}", layer.join(" ")));
            for id in &layer {
                signal_tree(id, Signal::SIGTERM);
            }
            let deadline = Instant::now() + STOP_WAIT;
            while self.kids.values().any(|k| layer.contains(&k.id)) && Instant::now() < deadline {
                self.reap_quietly();
                self.pump_logs();
                std::thread::sleep(Duration::from_millis(50));
            }
            for id in &layer {
                kill_cgroup(id);
            }
            self.reap_quietly();
        }
    }

    fn reap_quietly(&mut self) {
        while let Ok(WaitStatus::Exited(pid, _) | WaitStatus::Signaled(pid, _, _)) =
            waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG))
        {
            if let Some(mut k) = self.kids.remove(&pid.as_raw()) {
                pump(&mut k);
            }
        }
    }

    fn running(&self, id: &str) -> bool {
        self.kids.values().any(|k| k.id == id)
    }

    fn start(&mut self, id: &str, spec: &Svc) {
        let program = program(spec);
        match spawn(id, spec) {
            Ok(kid) => {
                let pid = kid.0;
                tel("init", "svc_start", json!({ "id": id, "pid": pid }));
                if spec.restart == SvcRestart::Never {
                    self.oneshot_done.insert(id.to_string());
                }
                self.kids.insert(
                    pid,
                    Kid {
                        id: id.to_string(),
                        spec: spec.clone(),
                        program,
                        started: Instant::now(),
                        out: Some(kid.1),
                        partial: Vec::new(),
                    },
                );
                self.write_actual(id, "running", Some(pid));
            }
            Err(e) => {
                log(&format!("{id} spawn: {e}"));
                tel("init", "svc_fail", json!({ "id": id, "err": e }));
                let why = format!("cannot start {}: {e}", spec.exec[0]);
                // Stamped like the service's own lines, so a later reader
                // can tell this start from the ones after a fix.
                append_log(id, format!("{} oath-init: {why}\n", now_rfc3339()).as_bytes());
                let h = self.history.entry(id.to_string()).or_default();
                h.quick += 1;
                h.why = Some(why.clone());
                self.failed.insert(id.to_string(), (spec.clone(), program));
                self.write_actual_why(id, "failed", None, Some(&why));
            }
        }
    }

    /// TERM the service's whole cgroup (only the main process when it
    /// keeps its children), wait, then KILL what is left.
    fn stop(&mut self, id: &str) {
        let pids: Vec<i32> =
            self.kids.iter().filter(|(_, k)| k.id == id).map(|(p, _)| *p).collect();
        let tree = !self.kids.values().any(|k| k.id == id && k.spec.keep_children);
        if tree {
            signal_tree(id, Signal::SIGTERM);
        } else {
            for p in &pids {
                let _ = kill(Pid::from_raw(*p), Signal::SIGTERM);
            }
        }
        let deadline = Instant::now() + STOP_WAIT;
        while Instant::now() < deadline
            && (pids.iter().any(|p| Path::new(&format!("/proc/{p}")).exists())
                || (tree && !cgroup_procs(id).is_empty()))
        {
            for p in &pids {
                let _ = waitpid(Pid::from_raw(*p), Some(WaitPidFlag::WNOHANG));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if tree {
            kill_cgroup(id);
        } else {
            for p in &pids {
                let _ = kill(Pid::from_raw(*p), Signal::SIGKILL);
            }
        }
        for p in &pids {
            if let Some(mut k) = self.kids.remove(p) {
                pump(&mut k);
            }
            let _ = waitpid(Pid::from_raw(*p), Some(WaitPidFlag::WNOHANG));
        }
        tel("init", "svc_stop", json!({ "id": id }));
        self.write_actual(id, "stopped", None);
    }

    fn write_actual(&self, id: &str, state: &str, pid: Option<i32>) {
        self.write_actual_why(id, state, pid, None)
    }

    /// Actual for `id`; a `failed` service says why in `reason`.
    fn write_actual_why(&self, id: &str, state: &str, pid: Option<i32>, why: Option<&str>) {
        let Ok(oid) = id.parse::<ObjectId>() else { return };
        if oid.kind != "svc" {
            return;
        }
        let dir = Path::new(DEFAULT_ROOT).join("objects/svc").join(&oid.name);
        // `oath rm` deleted it; do not bring back a half object.
        if !dir.join("desired.json").is_file() {
            return;
        }
        let h = self.history.get(id);
        let mut v = json!({
            "state": state,
            "pid": pid,
            "restarts": h.map(|h| h.restarts).unwrap_or(0),
            "last_exit": h.and_then(|h| h.last_exit).map(Exit::json),
            "since": now_rfc3339(),
        });
        if let Some(why) = why {
            v["reason"] = json!(why);
        }
        let running = pid.and_then(|p| self.kids.get(&p));
        if let Some((name, hash)) = running.and_then(|k| k.program.as_ref()) {
            v["pkg"] = json!({ "id": format!("pkg:{name}"), "hash": hash });
        }
        // A service's own record of its last run (backup-send writes one)
        // lives with its log, so it outlives the status tmpfs.
        let last = Path::new(LOG_ROOT).join("svc").join(format!("{}.last.json", oid.name));
        if let Ok(extra) = oath_core::read_json::<Value>(&last) {
            if let Some(obj) = extra.as_object() {
                for (k, val) in obj {
                    if v.get(k).is_none() {
                        v[k] = val.clone();
                    }
                }
            }
        }
        let _ = oath_core::write_json(&oath_core::status_path(Path::new(DEFAULT_ROOT), &oid), &v);
    }
}

fn program(spec: &Svc) -> Program {
    let exe = spec.exec.first()?;
    oath_core::program_realization(Path::new(DEFAULT_ROOT), Path::new(exe))
}

/// Applied service specs, plus the DHCP client when `net:net0` is dhcp,
/// and the ids whose spec could not be read.
fn wanted() -> (HashMap<String, Svc>, Vec<String>) {
    let mut out = HashMap::new();
    let mut unreadable = Vec::new();
    let Ok(cat) = Catalog::open(DEFAULT_ROOT) else { return (out, unreadable) };
    for id in cat.ls(Some("svc")).unwrap_or_default() {
        let dir = Path::new(DEFAULT_ROOT).join("objects/svc").join(&id.name);
        let applied_p = dir.join("applied.json");
        let path = if applied_p.is_file() { applied_p } else { dir.join("desired.json") };
        match oath_core::read_json::<Svc>(&path) {
            Ok(spec) => {
                out.insert(id.to_string(), spec);
            }
            Err(e) => {
                log(&format!("{id}: cannot read {}: {e}", path.display()));
                tel("init", "svc_spec", json!({ "id": id.to_string(), "ok": false }));
                unreadable.push(id.to_string());
            }
        }
    }
    if let Ok(obj) = cat.get(&ObjectId::new("net", "net0")) {
        if let Some(net) = applied::<oath_core::Net>(&cat, &obj) {
            if net.up && net.ipv4 == "dhcp" {
                out.insert(DHCP_ID.into(), dhcp_spec());
            }
        }
    }
    (out, unreadable)
}

fn dhcp_spec() -> Svc {
    let mut exec = vec!["/bin/dhcpcd".to_string()];
    exec.extend(oath_core::dhcpcd_args("net0"));
    Svc {
        exec,
        wants: Vec::new(),
        restart: SvcRestart::Always,
        enabled: true,
        user: None,
        env: Default::default(),
        bind_low_ports: false,
        keep_children: false,
        rtprio: None,
    }
}

/// `host:local` env plus `TZ`, from the applied host.
fn host_env() -> Vec<(String, String)> {
    let Ok(cat) = Catalog::open(DEFAULT_ROOT) else { return Vec::new() };
    let Ok(obj) = cat.get(&ObjectId::new("host", "local")) else { return Vec::new() };
    let Some(host) = applied::<Host>(&cat, &obj) else { return Vec::new() };
    oath_core::seat::login_env(&host)
        .into_iter()
        .filter(|(k, v)| oath_core::seat::valid_env_name(k) && !v.contains('\n'))
        .collect()
}

fn spawn(id: &str, spec: &Svc) -> Result<(i32, fs::File), String> {
    let plan = oath_core::svc_spawn(id, spec);
    let mut cmd = Command::new(&spec.exec[0]);
    cmd.args(&spec.exec[1..]);
    let xdg = plan.xdg_runtime_dir.clone();
    let _ = fs::create_dir_all(&xdg);
    if plan.cred.user != oath_core::SvcUser::Root {
        let _ = std::os::unix::fs::chown(&xdg, Some(plan.cred.uid), Some(plan.cred.gid));
        let _ = fs::set_permissions(&xdg, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    }
    cmd.env_clear()
        .env("PATH", "/bin")
        .env("HOME", plan.home)
        .env("USER", plan.user_name)
        .env("LOGNAME", plan.user_name)
        .env("SHELL", plan.shell)
        .env("XDG_RUNTIME_DIR", &xdg);
    for (k, v) in host_env() {
        cmd.env(k, v);
    }
    // Service env wins over host:local.env and the account defaults.
    for (k, v) in oath_core::svc_env_pairs(spec) {
        cmd.env(k, v);
    }
    let (read, write) = pipe().map_err(|e| format!("pipe: {e}"))?;
    let write2 = write.try_clone().map_err(|e| format!("pipe: {e}"))?;
    cmd.stdin(Stdio::null()).stdout(Stdio::from(write)).stderr(Stdio::from(write2));
    let cg = cgroup_dir(id);
    let _ = fs::create_dir_all(&cg);
    let procs = CString::new(cg.join("cgroup.procs").to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    let cred = plan.cred.clone();
    unsafe {
        cmd.pre_exec(move || {
            libc::setsid();
            // Join the service's cgroup while still root.
            let fd = libc::open(procs.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd >= 0 {
                libc::write(fd, b"0\n".as_ptr().cast(), 2);
                libc::close(fd);
            }
            oath_core::apply_svc_cred(&cred)
        });
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    drop(cmd);
    tel(
        "init",
        "svc_spawn",
        json!({ "id": id, "uid": plan.cred.uid, "user": plan.cred.user.as_str() }),
    );
    Ok((child.id() as i32, read))
}

fn pipe() -> std::io::Result<(fs::File, OwnedFd)> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let read = unsafe { fs::File::from_raw_fd(fds[0]) };
    let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    unsafe {
        let fl = libc::fcntl(read.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(read.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
    }
    Ok((read, write))
}

/// Read what the child wrote; whole lines go to its log with a UTC stamp.
fn pump(kid: &mut Kid) {
    let Some(out) = kid.out.as_mut() else { return };
    let mut buf = [0u8; 8192];
    loop {
        match out.read(&mut buf) {
            Ok(0) => {
                kid.out = None;
                break;
            }
            Ok(n) => kid.partial.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                break
            }
            Err(_) => {
                kid.out = None;
                break;
            }
        }
    }
    let ends = kid.partial.iter().rposition(|b| *b == b'\n');
    let flush_all = kid.out.is_none() && !kid.partial.is_empty();
    let take = match (ends, flush_all) {
        (Some(i), _) => i + 1,
        (None, true) => kid.partial.len(),
        (None, false) => return,
    };
    let chunk: Vec<u8> = kid.partial.drain(..take).collect();
    let stamp = now_rfc3339();
    let mut out = Vec::with_capacity(chunk.len() + 32);
    for line in chunk.split_inclusive(|b| *b == b'\n') {
        out.extend_from_slice(stamp.as_bytes());
        out.push(b' ');
        out.extend_from_slice(line);
        if !line.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    append_log(&kid.id, &out);
}

/// `/oath/log/<kind>/<name>.log` for an id, `/oath/log/init.log` for PID 1.
pub fn log_path(id: &str) -> PathBuf {
    match id.split_once(':') {
        Some((kind, name)) => Path::new(LOG_ROOT).join(kind).join(format!("{name}.log")),
        None => Path::new(LOG_ROOT).join(format!("{id}.log")),
    }
}

/// Append to a log, rotating to `.1` past 256 KiB (one old file kept).
pub fn append_log(id: &str, bytes: &[u8]) {
    let path = log_path(id);
    if let Some(d) = path.parent() {
        if !Path::new(LOG_ROOT).is_dir() {
            return;
        }
        let _ = fs::create_dir_all(d);
    }
    if fs::metadata(&path).map(|m| m.len() > LOG_MAX).unwrap_or(false) {
        let mut old = path.clone().into_os_string();
        old.push(".1");
        let _ = fs::rename(&path, old);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(bytes);
    }
}

fn cgroup_dir(id: &str) -> PathBuf {
    Path::new(CGROUP_ROOT).join(id.replace(':', "-"))
}

fn cgroup_procs(id: &str) -> Vec<i32> {
    fs::read_to_string(cgroup_dir(id).join("cgroup.procs"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

fn signal_tree(id: &str, sig: Signal) {
    for p in cgroup_procs(id) {
        let _ = kill(Pid::from_raw(p), sig);
    }
}

/// SIGKILL everything left in the service's cgroup, then remove it.
fn kill_cgroup(id: &str) {
    let dir = cgroup_dir(id);
    if fs::write(dir.join("cgroup.kill"), b"1").is_err() {
        signal_tree(id, Signal::SIGKILL);
    }
    for _ in 0..40 {
        if cgroup_procs(id).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = fs::remove_dir(&dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_paths() {
        assert_eq!(log_path("svc:sshd"), Path::new("/oath/log/svc/sshd.log"));
        assert_eq!(log_path("net:net0"), Path::new("/oath/log/net/net0.log"));
        assert_eq!(log_path("init"), Path::new("/oath/log/init.log"));
    }

    #[test]
    fn state_survives_the_handoff() {
        let spec: Svc = serde_json::from_value(json!({"exec": ["/bin/sleep", "9"]})).unwrap();
        let mut sup = Supervisor::default();
        sup.pending.push(Pending {
            id: "svc:a".into(),
            spec: spec.clone(),
            due: Instant::now() + Duration::from_secs(4),
        });
        let prog = Some(("sleep".to_string(), "sha256-0".to_string()));
        sup.failed.insert("svc:b".into(), (spec.clone(), prog.clone()));
        sup.history.insert(
            "svc:b".into(),
            History {
                restarts: 2,
                quick: 5,
                last_exit: Some(Exit::Code(3)),
                why: Some("x".into()),
            },
        );
        sup.oneshot_done.insert("svc:c".into());
        sup.tasks.insert(42, "verify".into());
        let text = serde_json::to_string(&sup.export()).unwrap();
        assert!(sup.pending.is_empty() && sup.failed.is_empty(), "export hands everything over");
        let back = Supervisor::import(serde_json::from_str(&text).unwrap());
        assert_eq!(back.pending[0].id, "svc:a");
        assert!(back.pending[0].due > Instant::now() + Duration::from_secs(3));
        assert_eq!(back.failed["svc:b"], (spec, prog));
        let h = &back.history["svc:b"];
        assert_eq!((h.restarts, h.quick, h.why.as_deref()), (2, 5, Some("x")));
        assert!(matches!(h.last_exit, Some(Exit::Code(3))));
        assert!(back.oneshot_done.contains("svc:c"));
        assert!(back.task_running("verify"));
    }

    #[test]
    fn exit_json() {
        assert_eq!(Exit::Code(3).json(), json!({"code": 3}));
        assert_eq!(Exit::Signal(9).json(), json!({"signal": 9}));
        assert!(Exit::Code(1).failed() && Exit::Signal(15).failed() && !Exit::Code(0).failed());
    }
}
