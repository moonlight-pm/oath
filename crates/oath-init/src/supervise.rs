//! PID 1's supervisor. It runs what was applied (`applied.json`) in
//! `wants` order, gives every service its own cgroup and log, restarts
//! with backoff, and gives up into `failed` after repeated quick crashes.
//! It also runs the DHCP client for `net:net0` when that is `dhcp`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CString;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use oath_core::login::Seat;
use oath_core::{now_rfc3339, tel, Catalog, Host, ObjectId, Svc, SvcRestart, DEFAULT_ROOT};
use serde_json::{json, Value};

use crate::{applied, log};

/// Service logs: `/oath/log/<kind>/<name>.log`, plus `.1` after rotation.
pub const LOG_ROOT: &str = "/oath/log";
const LOG_MAX: u64 = 256 * 1024;
/// A log's flood guard ([`Gate`]): past this many lines or bytes in one
/// window, the window's other lines are held back and counted. The busiest
/// window measured on five machines in normal running was about 1,300 lines
/// or 270 KB (blocky's start, a kernel log at boot).
const BURST_LINES: usize = 5_000;
const BURST_BYTES: usize = 512 * 1024;
const BURST_WINDOW: Duration = Duration::from_secs(30);
/// Of the lines a guard held back, the newest this many bytes are written
/// when the window ends or the service exits: the end of a flood is often
/// why it ended (an abort's message, a panic's trace).
const HELD_MOST: usize = 32 * 1024;
/// A run this long with no newline is a line of its own.
const LINE_MOST: usize = 16 * 1024;
/// What one pump takes from a pipe before it writes, so a writer that
/// never pauses cannot keep PID 1 reading.
const READ_MOST: usize = 1024 * 1024;
/// Each log pipe's buffer. PID 1 reads a pipe within [`GATHER`] of a
/// write, so a writer waits on it only past about 100 MB/s, or while PID 1
/// is busy (a stop waits up to 5 s).
const PIPE_SIZE: i32 = 1024 * 1024;
/// How long a burst of output gathers before PID 1 reads it.
const GATHER: Duration = Duration::from_millis(10);
/// `<name>.exit.log` keeps this much of the log before each failed exit,
/// for the last [`EXITS_KEPT`] of them.
const EXIT_TAIL: usize = 64 * 1024;
const EXITS_KEPT: usize = 3;
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
    /// The program its package moved to while an update left it running
    /// (it has a `seat`): it takes that one when it next starts.
    deferred: Program,
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

/// The log pipe a `keep_children` service's children still hold after the
/// main process they came from has gone. PID 1 goes on reading it: with no
/// reader, their next write would kill them with SIGPIPE. It ends with them.
struct Survivors {
    id: String,
    out: Option<fs::File>,
    partial: Vec<u8>,
}

/// A scheduled service (R48): its spec and its next start.
#[derive(Clone)]
struct Sched {
    spec: Svc,
    next: Option<DateTime<Local>>,
}

#[derive(Default)]
pub struct Supervisor {
    kids: HashMap<i32, Kid>,
    oneshot_done: HashSet<String>,
    pending: Vec<Pending>,
    survivors: Vec<Survivors>,
    /// Scheduled services (`schedule`), by id: PID 1 starts each at its
    /// next match.
    schedules: HashMap<String, Sched>,
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
    /// Each log's flood guard, by id. Not handed over: a resumed PID 1
    /// starts each count afresh.
    gates: HashMap<String, Gate>,
}

/// Everything the supervisor knows, for a PID 1 that execs itself after a
/// panic or into a new binary (`crate::resume`). Children stay PID 1's
/// across the exec; their log pipes stay open (the fd numbers are here).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    kids: Vec<KidState>,
    oneshot_done: Vec<String>,
    pending: Vec<(String, Svc, u64)>,
    failed: Vec<(String, Svc, Program)>,
    history: Vec<(String, u32, u32, Option<Exit>, Option<String>)>,
    tasks: Vec<(i32, String)>,
    /// Scheduled services and their next start (RFC 3339).
    #[serde(default)]
    schedules: Vec<(String, Svc, Option<String>)>,
    /// `Survivors`: the id, the pipe's fd and what it has not logged yet.
    #[serde(default)]
    survivors: Vec<(String, i32, Vec<u8>)>,
}

impl State {
    /// Service processes in it.
    pub fn len(&self) -> usize {
        self.kids.len()
    }

    /// The log pipes it names, which the exec must let through.
    pub fn fds(&self) -> Vec<i32> {
        self.kids.iter().filter_map(|k| k.out).chain(self.survivors.iter().map(|s| s.1)).collect()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct KidState {
    pid: i32,
    id: String,
    spec: Svc,
    #[serde(default)]
    program: Program,
    #[serde(default)]
    deferred: Program,
    /// Milliseconds it had run.
    ran_ms: u64,
    out: Option<i32>,
    partial: Vec<u8>,
}

impl Supervisor {
    /// Hand everything over; the caller lets the log pipes through the exec
    /// ([`State::fds`]), and [`Self::import`] takes them back.
    pub fn export(&mut self) -> State {
        let now = Instant::now();
        let kids = self
            .kids
            .drain()
            .map(|(pid, k)| {
                let out = k.out.map(std::os::fd::IntoRawFd::into_raw_fd);
                KidState {
                    pid,
                    id: k.id,
                    spec: k.spec,
                    program: k.program,
                    deferred: k.deferred,
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
            schedules: self
                .schedules
                .drain()
                .map(|(id, s)| (id, s.spec, s.next.map(|t| t.to_rfc3339())))
                .collect(),
            survivors: self
                .survivors
                .drain(..)
                .filter_map(|s| {
                    Some((s.id, std::os::fd::IntoRawFd::into_raw_fd(s.out?), s.partial))
                })
                .collect(),
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
                    deferred: k.deferred,
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
        s.schedules = state
            .schedules
            .into_iter()
            .map(|(id, spec, next)| {
                let next = next
                    .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                    .map(|t| t.with_timezone(&Local));
                (id, Sched { spec, next })
            })
            .collect();
        s.survivors = state
            .survivors
            .into_iter()
            .map(|(id, fd, partial)| {
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
                Survivors { id, out: Some(unsafe { fs::File::from_raw_fd(fd) }), partial }
            })
            .collect();
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
        self.converge_keeping(false)
    }

    /// [`Self::converge`]; with `keep_seat` (an update's apply, `converge
    /// keep-seat` on `init.sock`) a running service with a `seat` keeps its
    /// program when its package moves, and takes the new one when it next
    /// starts: restarting the desk ends a person's session.
    pub fn converge_keeping(&mut self, keep_seat: bool) {
        let (mut wanted, unreadable) = wanted(Path::new(DEFAULT_ROOT));
        // A spec PID 1 cannot read leaves that service as it is.
        for id in unreadable {
            if let Some(k) = self.kids.values().find(|k| k.id == id) {
                wanted.insert(id, k.spec.clone());
            }
        }
        // The seat (R42): a service of the other owner's turn is held, as
        // if disabled, until the seat changes hands.
        let seat = oath_core::login::Seat::current();
        let mut held = HashSet::new();
        for (id, spec) in wanted.iter_mut() {
            if spec.enabled && spec.seat.is_some_and(|s| !s.runs_on(seat)) {
                spec.enabled = false;
                held.insert(id.clone());
            }
        }

        let running: Vec<(String, Svc, Program)> =
            self.kids.values().map(|k| (k.id.clone(), k.spec.clone(), k.program.clone())).collect();
        for (id, spec, was) in running {
            let Some(want) =
                wanted.get(&id).filter(|s| s.enabled && !s.exec.is_empty() && s.same_spawn(&spec))
            else {
                // It stops by the catalog's word on its children, not the
                // word it started with.
                if let Some(want) = wanted.get(&id) {
                    if let Some(kid) = self.kids.values_mut().find(|k| k.id == id) {
                        kid.spec.keep_children = want.keep_children;
                    }
                }
                self.stop(&id);
                continue;
            };
            // What takes no new process (`keep_children`, `restart`, `wants`)
            // reaches the running one at once.
            if let Some(kid) = self.kids.values_mut().find(|k| k.id == id) {
                kid.spec.clone_from(want);
            }
            // A service follows its package: an apply that moved the
            // realization its program resolves to restarts it.
            let now = program(want);
            if let (Some((name, hash)), true) = (now.clone(), id.starts_with("svc:")) {
                if was.as_ref().is_some_and(|w| *w != (name.clone(), hash.clone())) {
                    let Some(kid) = self.kids.values_mut().find(|k| k.id == id) else { continue };
                    if kid.deferred == now {
                        continue;
                    }
                    if keep_seat && want.seat.is_some() {
                        kid.deferred = now;
                        let why = format!(
                            "pkg:{name} moved to {hash} in an update; this keeps its program \
                             until it next starts (a reboot, or the seat changing hands)"
                        );
                        log(&format!("{id}: {why}"));
                        tel("init", "svc_defer", json!({ "id": id, "pkg": name, "hash": hash }));
                        let pid = self.kids.iter().find(|(_, k)| k.id == id).map(|(p, _)| *p);
                        self.write_actual_why(&id, "running", pid, Some(&why));
                        continue;
                    }
                    log(&format!("{id}: {} is now pkg:{name} {hash}; restarting", want.exec[0]));
                    tel("init", "svc_follow", json!({ "id": id, "pkg": name, "hash": hash }));
                    self.stop(&id);
                }
            }
        }
        self.pending
            .retain(|p| wanted.get(&p.id).is_some_and(|s| s.enabled && s.same_spawn(&p.spec)));
        for p in &mut self.pending {
            if let Some(want) = wanted.get(&p.id) {
                p.spec.clone_from(want);
            }
        }
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

        // Scheduled services (R48): only those still applied, enabled and
        // scheduled keep a schedule.
        self.schedules
            .retain(|id, _| wanted.get(id).is_some_and(|s| s.enabled && s.schedule.is_some()));
        set_tz();

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
            // It starts at its times, not now.
            if spec.schedule.is_some() {
                self.plan(id, spec);
                continue;
            }
            if spec.exec.is_empty() || self.running(id) || self.pending.iter().any(|p| &p.id == id)
            {
                continue;
            }
            if self.failed.contains_key(id) {
                let why = self.history.get(id.as_str()).and_then(|h| h.why.clone());
                self.write_actual_why(id, "failed", None, why.as_deref());
                continue;
            }
            let last = self.history.get(id.as_str()).and_then(|h| h.last_exit);
            if finished(spec, self.oneshot_done.contains(id), last) {
                continue;
            }
            self.start(id, spec);
        }
        for (id, spec) in &wanted {
            if spec.enabled {
                continue;
            }
            self.oneshot_done.remove(id);
            if self.running(id) {
                continue;
            }
            if held.contains(id) {
                let why = match seat {
                    Seat::Desk => "the desk has the seat; this runs while the console has it",
                    Seat::Console => "the console has the seat; `oath desk` gives it to the desk",
                };
                self.write_actual_why(id, "held", None, Some(why));
            } else {
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
            let g = gate(&mut self.gates, &kid.id);
            pump(&mut kid, g);
            self.keep_pipe(&mut kid);
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
            let again = starts_again(kid.spec.restart, exit);
            // A run its `restart` does not follow (`never`, or `on-failure`
            // after exit 0) is finished: converge leaves it down.
            if !again {
                self.oneshot_done.insert(kid.id.clone());
            }
            if !kid.spec.keep_children {
                kill_cgroup(&kid.id);
            }
            if !again {
                self.note_exit(&kid.id, exit, ran, "not restarted");
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
                self.note_exit(&kid.id, exit, ran, &why);
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
                let then = format!("restart #{} in {}s", restarts + 1, delay.as_secs());
                log(&format!("{}: exit {exit:?}, {then}", kid.id));
                self.note_exit(&kid.id, exit, ran, &then);
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

    /// When a scheduled service next starts: its next match, or now when a
    /// match passed since its last run (the machine was down at 04:00).
    fn plan(&mut self, id: &str, spec: &Svc) {
        let Some(pattern) = spec.schedule.clone() else { return };
        let same = self.schedules.get(id).is_some_and(|s| s.spec.schedule == spec.schedule);
        if same {
            if let Some(s) = self.schedules.get_mut(id) {
                s.spec = spec.clone();
            }
        } else {
            let now = Local::now();
            let next = if oath_core::schedule::owed(&pattern, last_run(id), &now) {
                log(&format!("{id}: a scheduled run passed while PID 1 was not watching: now"));
                Some(now)
            } else {
                oath_core::schedule::next_after(&pattern, &now)
            };
            if next.is_none() {
                log(&format!("{id}: schedule {pattern:?} has no next time"));
            }
            self.schedules.insert(id.to_string(), Sched { spec: spec.clone(), next });
        }
        if !self.running(id) {
            self.write_actual(id, "scheduled", None);
        }
    }

    /// Start scheduled services whose time has come; one still running
    /// from its last time is skipped, and says so.
    fn start_scheduled(&mut self) {
        let now = Local::now();
        let due: Vec<String> = self
            .schedules
            .iter()
            .filter(|(_, s)| s.next.is_some_and(|n| n <= now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            let Some(s) = self.schedules.get(&id).cloned() else { continue };
            if self.running(&id) {
                let at = now.to_rfc3339();
                log(&format!("{id}: its run at {at} is skipped: the last one is still running"));
            } else {
                record_run(&id, &now);
                self.start(&id, &s.spec);
            }
            let pattern = s.spec.schedule.clone().unwrap_or_default();
            let next = oath_core::schedule::next_after(&pattern, &now);
            if let Some(e) = self.schedules.get_mut(&id) {
                e.next = next;
            }
        }
    }

    /// Start restarts whose backoff is over, and scheduled services whose
    /// time has come.
    pub fn start_due(&mut self) {
        self.start_scheduled();
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
            pump(kid, gate(&mut self.gates, &kid.id));
        }
        for s in &mut self.survivors {
            pump_pipe(&s.id, &mut s.out, &mut s.partial, gate(&mut self.gates, &s.id));
        }
        self.survivors.retain(|s| s.out.is_some());
        // A window that ended in quiet still says what it held back.
        let (stamp, now) = (now_rfc3339(), Instant::now());
        for (id, gate) in &mut self.gates {
            let mut text = Vec::new();
            gate.roll(&stamp, now, &mut text);
            append_log(id, &text);
        }
    }

    /// Read what services write as it comes, until `until`. A pipe PID 1
    /// leaves unread fills, and then its writer waits: a compositor or a
    /// window manager that waits stops drawing.
    pub fn pump_until(&mut self, until: Instant) {
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || !self.wait_output(left) {
                return;
            }
            // A burst gathers into one write.
            std::thread::sleep(GATHER.min(left));
            self.pump_logs();
        }
    }

    /// Wait up to `most` for a service to write; false when none did.
    fn wait_output(&self, most: Duration) -> bool {
        let mut fds: Vec<libc::pollfd> = self
            .kids
            .values()
            .filter_map(|k| k.out.as_ref())
            .chain(self.survivors.iter().filter_map(|s| s.out.as_ref()))
            .map(|f| libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 })
            .collect();
        let ms = most.as_millis().min(i32::MAX as u128) as i32;
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms) > 0 }
    }

    /// Say in the service's log that it exited, after what its guard held
    /// back. A failed exit also keeps the lines before it in
    /// `<name>.exit.log`, where the restart's output cannot rotate them away.
    fn note_exit(&mut self, id: &str, exit: Exit, ran: Duration, then: &str) {
        let stamp = now_rfc3339();
        let mut text = Vec::new();
        gate(&mut self.gates, id).close(&stamp, &mut text);
        append_log(id, &text);
        let what = format!("exit {exit:?} after {} s; {then}", ran.as_secs());
        let mut line = format!("{stamp} oath-init: {what}");
        if exit.failed() {
            keep_exit(&log_path(id), &exit_log_path(id), &format!("== {stamp} {id} {what}"));
            line.push_str(&format!(" (the lines before it: oath logs {id} --exits)"));
        }
        line.push('\n');
        append_log(id, line.as_bytes());
    }

    /// A main process is gone: if its children stay, so does their log.
    fn keep_pipe(&mut self, kid: &mut Kid) {
        if !kid.spec.keep_children {
            return;
        }
        if let Some(out) = kid.out.take() {
            self.survivors.push(Survivors {
                id: kid.id.clone(),
                out: Some(out),
                partial: std::mem::take(&mut kid.partial),
            });
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
                let g = gate(&mut self.gates, &k.id);
                pump(&mut k, g);
            }
        }
    }

    fn running(&self, id: &str) -> bool {
        self.kids.values().any(|k| k.id == id)
    }

    fn start(&mut self, id: &str, spec: &Svc) {
        crate::splash::say(&format!("Starting svc:{id}"));
        let program = program(spec);
        match spawn(id, spec) {
            Ok(kid) => {
                let pid = kid.0;
                tel("init", "svc_start", json!({ "id": id, "pid": pid }));
                // A oneshot runs once a boot; any other run that starts is no
                // longer finished.
                if spec.restart == SvcRestart::Never {
                    self.oneshot_done.insert(id.to_string());
                } else {
                    self.oneshot_done.remove(id);
                }
                self.kids.insert(
                    pid,
                    Kid {
                        id: id.to_string(),
                        spec: spec.clone(),
                        program,
                        deferred: None,
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
            // The other services go on writing while this one stops.
            self.pump_logs();
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
                let g = gate(&mut self.gates, &k.id);
                pump(&mut k, g);
                self.keep_pipe(&mut k);
            }
            let _ = waitpid(Pid::from_raw(*p), Some(WaitPidFlag::WNOHANG));
        }
        // Its next run's first line is not a repeat of this run's last.
        let mut text = Vec::new();
        gate(&mut self.gates, id).close(&now_rfc3339(), &mut text);
        append_log(id, &text);
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
        // A scheduled service between its runs waits for its next time.
        let sched = self.schedules.get(id);
        let state = if sched.is_some() && state == "stopped" { "scheduled" } else { state };
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
        if let Some(s) = sched {
            v["next"] = json!(s.next.map(|t| t.to_rfc3339()));
            v["last_run"] = json!(last_run(id).map(|t| t.to_rfc3339()));
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

/// Whether a service that ended with `exit` starts again, by its `restart`.
fn starts_again(restart: SvcRestart, exit: Exit) -> bool {
    match restart {
        SvcRestart::Never => false,
        SvcRestart::Always => true,
        SvcRestart::OnFailure => exit.failed(),
    }
}

/// Whether converge leaves a service down. `done` is a oneshot that started
/// this boot, or a run its `restart` did not follow; `last` is how its last
/// run ended. It stays down until the next boot, an apply of
/// `enabled=false` and then `enabled=true`, or a `restart` that would have
/// started it again.
fn finished(spec: &Svc, done: bool, last: Option<Exit>) -> bool {
    done && match spec.restart {
        SvcRestart::Never => true,
        SvcRestart::Always => false,
        SvcRestart::OnFailure => last.is_some_and(|e| !e.failed()),
    }
}

/// Where a scheduled service's last run is kept, beside its log: it
/// outlives the status tmpfs, so a run owed across a reboot is known.
fn schedule_file(id: &str) -> Option<PathBuf> {
    let oid: ObjectId = id.parse().ok()?;
    Some(Path::new(LOG_ROOT).join("svc").join(format!("{}.schedule.json", oid.name)))
}

fn last_run(id: &str) -> Option<DateTime<Local>> {
    let v: Value = oath_core::read_json(&schedule_file(id)?).ok()?;
    let t = DateTime::parse_from_rfc3339(v["last_run"].as_str()?).ok()?;
    Some(t.with_timezone(&Local))
}

fn record_run(id: &str, at: &DateTime<Local>) {
    if let Some(f) = schedule_file(id) {
        let _ = fs::create_dir_all(f.parent().unwrap_or(Path::new(LOG_ROOT)));
        let _ = oath_core::write_json(&f, &json!({ "last_run": at.to_rfc3339() }));
    }
}

/// PID 1's own `TZ`, so a schedule is the machine's local time: the
/// applied `host:local.timezone`, unless `host:local.env` sets `TZ`.
fn set_tz() {
    let p = Path::new(DEFAULT_ROOT).join("objects/host/local/applied.json");
    let Ok(host) = oath_core::read_json::<Host>(&p) else { return };
    let tz = host.env.get("TZ").cloned().unwrap_or(host.timezone);
    if !tz.is_empty() && std::env::var("TZ").ok().as_deref() != Some(tz.as_str()) {
        std::env::set_var("TZ", tz);
    }
}

/// Applied service specs, plus the DHCP client when `net:net0` is dhcp,
/// and the ids whose spec could not be read. A service with no
/// `applied.json` was never applied (`oath new` made it): it does not run.
fn wanted(root: &Path) -> (HashMap<String, Svc>, Vec<String>) {
    let mut out = HashMap::new();
    let mut unreadable = Vec::new();
    let Ok(cat) = Catalog::open(root) else { return (out, unreadable) };
    for id in cat.ls(Some("svc")).unwrap_or_default() {
        let path = root.join("objects/svc").join(&id.name).join("applied.json");
        if !path.is_file() {
            continue;
        }
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
        seat: None,
        schedule: None,
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
        libc::fcntl(read.as_raw_fd(), libc::F_SETPIPE_SZ, PIPE_SIZE);
    }
    Ok((read, write))
}

/// Read what the child wrote; whole lines go to its log with a UTC stamp.
fn pump(kid: &mut Kid, gate: &mut Gate) {
    pump_pipe(&kid.id, &mut kid.out, &mut kid.partial, gate);
}

fn pump_pipe(id: &str, pipe: &mut Option<fs::File>, partial: &mut Vec<u8>, gate: &mut Gate) {
    let Some(out) = pipe.as_mut() else { return };
    let mut buf = [0u8; 64 * 1024];
    let mut took = 0;
    while took < READ_MOST {
        match out.read(&mut buf) {
            Ok(0) => {
                *pipe = None;
                break;
            }
            Ok(n) => {
                partial.extend_from_slice(&buf[..n]);
                took += n;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                break
            }
            Err(_) => {
                *pipe = None;
                break;
            }
        }
    }
    let text = take_lines(partial, pipe.is_none(), gate, &now_rfc3339(), Instant::now());
    append_log(id, &text);
}

/// The whole lines in `partial`, through the guard, as log text; all of it
/// once the pipe is `closed`. A run of [`LINE_MOST`] bytes with no newline
/// is a line of its own, so PID 1 never holds more of one.
fn take_lines(
    partial: &mut Vec<u8>,
    closed: bool,
    gate: &mut Gate,
    stamp: &str,
    now: Instant,
) -> Vec<u8> {
    let mut text = Vec::new();
    let mut at = 0;
    while at < partial.len() {
        let rest = &partial[at..];
        let (line, used) = match rest.iter().take(LINE_MOST).position(|b| *b == b'\n') {
            Some(i) => (&rest[..i], i + 1),
            None if rest.len() >= LINE_MOST => (&rest[..LINE_MOST], LINE_MOST),
            None if closed => (rest, rest.len()),
            None => break,
        };
        gate.line(line, stamp, now, &mut text);
        at += used;
    }
    partial.drain(..at);
    text
}

/// A log's flood guard. A line the same as the one before it is counted,
/// not written. Past [`BURST_LINES`] lines or [`BURST_BYTES`] in a
/// [`BURST_WINDOW`], lines are held back: counted, and the newest
/// [`HELD_MOST`] bytes of them kept. When the window ends or the service
/// exits, the guard writes the lines it kept and one line of its own for
/// each count. PID 1 reads every line all the same, so the writer never
/// waits on a flood.
#[derive(Default)]
struct Gate {
    /// When the window began.
    since: Option<Instant>,
    /// Lines and bytes written in it.
    lines: usize,
    bytes: usize,
    /// The last line read, and whether it was written.
    last: Option<Vec<u8>>,
    last_written: bool,
    /// Copies of `last` read since it was written.
    repeats: u64,
    /// Lines held back since the last count, from the stamp of the first.
    held: u64,
    held_bytes: u64,
    held_from: String,
    /// The newest of them, with their stamps.
    kept: VecDeque<(String, Vec<u8>)>,
    kept_bytes: usize,
}

impl Gate {
    /// One line (no newline) read at `stamp`; what to write goes to `out`.
    fn line(&mut self, line: &[u8], stamp: &str, now: Instant, out: &mut Vec<u8>) {
        self.roll(stamp, now, out);
        if self.last.as_deref() == Some(line) {
            if self.last_written {
                self.repeats += 1;
            } else {
                self.count(1, line.len(), stamp);
            }
            return;
        }
        self.say_repeats(stamp, out);
        self.last_written = self.write(line, stamp, out);
        if !self.last_written {
            self.count(1, line.len(), stamp);
            self.keep(line, stamp);
        }
        self.last = Some(line.to_vec());
    }

    /// Write a line if the window has room for it. Once it has held one
    /// back, it holds back the rest, so what it writes stays in order.
    fn write(&mut self, line: &[u8], stamp: &str, out: &mut Vec<u8>) -> bool {
        if self.held > 0 || self.lines >= BURST_LINES || self.bytes + line.len() + 1 > BURST_BYTES
        {
            return false;
        }
        self.lines += 1;
        self.bytes += line.len() + 1;
        stamp_line(out, stamp, line);
        true
    }

    /// Count `n` lines of `len` bytes held back.
    fn count(&mut self, n: u64, len: usize, stamp: &str) {
        if self.held == 0 {
            self.held_from = stamp.to_string();
        }
        self.held += n;
        self.held_bytes += n * (len as u64 + 1);
    }

    /// Keep a line held back among the newest [`HELD_MOST`] bytes of them.
    fn keep(&mut self, line: &[u8], stamp: &str) {
        self.kept.push_back((stamp.to_string(), line.to_vec()));
        self.kept_bytes += line.len() + 1;
        while self.kept_bytes > HELD_MOST {
            let Some((_, l)) = self.kept.pop_front() else { break };
            self.kept_bytes -= l.len() + 1;
        }
    }

    fn say_repeats(&mut self, stamp: &str, out: &mut Vec<u8>) {
        if self.repeats == 0 {
            return;
        }
        let last = self.last.take().unwrap_or_default();
        let shown = String::from_utf8_lossy(&last[..last.len().min(160)]);
        let more = if last.len() > 160 { "…" } else { "" };
        let note = format!("oath-init: repeated {} more times: {shown}{more}", self.repeats);
        if !self.write(note.as_bytes(), stamp, out) {
            // No room to say it: they are held back, as any line would be.
            self.count(self.repeats, last.len(), stamp);
        }
        self.repeats = 0;
        self.last = Some(last);
    }

    /// Past the window, say what it held back and open a new one.
    fn roll(&mut self, stamp: &str, now: Instant, out: &mut Vec<u8>) {
        if self.since.is_some_and(|t| now.duration_since(t) < BURST_WINDOW) {
            return;
        }
        self.since = Some(now);
        self.lines = 0;
        self.bytes = 0;
        self.say(stamp, out);
    }

    /// Write the lines it kept, then its counts.
    fn say(&mut self, stamp: &str, out: &mut Vec<u8>) {
        if self.held > 0 {
            let kept = self.kept.len();
            for (s, l) in self.kept.drain(..) {
                stamp_line(out, &s, &l);
            }
            let note = format!(
                "oath-init: held back {} lines ({} KiB) since {}, past {BURST_LINES} lines or \
                 {} KiB in {} s; the last {kept} of them are above",
                self.held,
                self.held_bytes.div_ceil(1024),
                self.held_from,
                BURST_BYTES / 1024,
                BURST_WINDOW.as_secs(),
            );
            stamp_line(out, stamp, note.as_bytes());
            self.held = 0;
            self.held_bytes = 0;
            self.kept_bytes = 0;
        }
        self.say_repeats(stamp, out);
    }

    /// The process exited or stopped: say everything, and its next run's
    /// first line is not a repeat.
    fn close(&mut self, stamp: &str, out: &mut Vec<u8>) {
        self.say(stamp, out);
        self.last = None;
        self.last_written = false;
    }
}

fn stamp_line(out: &mut Vec<u8>, stamp: &str, line: &[u8]) {
    out.extend_from_slice(stamp.as_bytes());
    out.push(b' ');
    out.extend_from_slice(line);
    out.push(b'\n');
}

/// The guard for log `id`.
fn gate<'a>(gates: &'a mut HashMap<String, Gate>, id: &str) -> &'a mut Gate {
    gates.entry(id.to_string()).or_default()
}

/// `/oath/log/<kind>/<name>.log` for an id, `/oath/log/init.log` for PID 1.
pub fn log_path(id: &str) -> PathBuf {
    match id.split_once(':') {
        Some((kind, name)) => Path::new(LOG_ROOT).join(kind).join(format!("{name}.log")),
        None => Path::new(LOG_ROOT).join(format!("{id}.log")),
    }
}

/// `/oath/log/<kind>/<name>.exit.log`: the lines before each of a service's
/// last [`EXITS_KEPT`] failed exits.
pub fn exit_log_path(id: &str) -> PathBuf {
    match id.split_once(':') {
        Some((kind, name)) => Path::new(LOG_ROOT).join(kind).join(format!("{name}.exit.log")),
        None => Path::new(LOG_ROOT).join(format!("{id}.exit.log")),
    }
}

/// `<log>.1`, where a log rotates to.
fn rotated(path: &Path) -> PathBuf {
    let mut old = path.as_os_str().to_owned();
    old.push(".1");
    PathBuf::from(old)
}

/// Append to a log, rotating to `.1` past 256 KiB (one old file kept).
pub fn append_log(id: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let path = log_path(id);
    if let Some(d) = path.parent() {
        if !Path::new(LOG_ROOT).is_dir() {
            return;
        }
        let _ = fs::create_dir_all(d);
    }
    if fs::metadata(&path).map(|m| m.len() > LOG_MAX).unwrap_or(false) {
        let _ = fs::rename(&path, rotated(&path));
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(bytes);
    }
}

/// Keep the last [`EXIT_TAIL`] of `log` (with its `.1`), whole lines, in
/// `exits`, ended by the line `end`, after the last [`EXITS_KEPT`] - 1 kept
/// there before. Each exit there ends with its own line starting `== `; a
/// log line starts with its stamp.
fn keep_exit(log: &Path, exits: &Path, end: &str) {
    let Some(dir) = exits.parent().filter(|d| d.is_dir()) else { return };
    let mut text = fs::read(rotated(log)).unwrap_or_default();
    text.extend(fs::read(log).unwrap_or_default());
    let mut tail = &text[text.len().saturating_sub(EXIT_TAIL)..];
    if tail.len() < text.len() {
        let first = tail.iter().position(|b| *b == b'\n').map_or(tail.len(), |i| i + 1);
        tail = &tail[first..];
    }
    let old = fs::read(exits).unwrap_or_default();
    let mut ends = Vec::new();
    let mut at = 0;
    for line in old.split_inclusive(|b| *b == b'\n') {
        at += line.len();
        if line.starts_with(b"== ") {
            ends.push(at);
        }
    }
    let from = if ends.len() >= EXITS_KEPT { ends[ends.len() - EXITS_KEPT] } else { 0 };
    let mut body = old[from..].to_vec();
    body.extend_from_slice(tail);
    if !body.is_empty() && !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    body.extend_from_slice(end.as_bytes());
    body.push(b'\n');
    let tmp = dir.join(format!(".{}.tmp", exits.file_name().unwrap_or_default().to_string_lossy()));
    if fs::write(&tmp, &body).is_ok() {
        let _ = fs::rename(&tmp, exits);
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
        let (read, write) = pipe().unwrap();
        sup.survivors.push(Survivors { id: "svc:d".into(), out: Some(read), partial: vec![b'x'] });
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
        // The pipe a kept child holds is still the same pipe.
        let mut back = back;
        assert_eq!(back.survivors[0].partial, vec![b'x']);
        let mut w = fs::File::from(write);
        std::io::Write::write_all(&mut w, b"y").unwrap();
        let mut got = [0u8; 1];
        back.survivors[0].out.as_mut().unwrap().read_exact(&mut got).unwrap();
        assert_eq!(&got, b"y");
    }

    #[test]
    fn a_kept_childs_pipe_stays_read_until_it_ends() {
        let kid = |keep: bool, out| Kid {
            id: "svc:a".into(),
            spec: serde_json::from_value(
                json!({"exec": ["/bin/sleep", "9"], "keep_children": keep}),
            )
            .unwrap(),
            program: None,
            deferred: None,
            started: Instant::now(),
            out: Some(out),
            partial: Vec::new(),
        };
        let mut sup = Supervisor::default();
        let (read, _write) = pipe().unwrap();
        let mut plain = kid(false, read);
        sup.keep_pipe(&mut plain);
        assert!(sup.survivors.is_empty() && plain.out.is_some(), "no kept children, no kept pipe");
        let (read, write) = pipe().unwrap();
        let mut kept = kid(true, read);
        sup.keep_pipe(&mut kept);
        assert!(kept.out.is_none());
        sup.pump_logs();
        assert_eq!(sup.survivors.len(), 1, "open while a child holds it");
        drop(write);
        sup.pump_logs();
        assert!(sup.survivors.is_empty(), "gone with the last child");
    }

    #[test]
    fn only_an_applied_service_runs() {
        let d = tempfile::tempdir().unwrap();
        oath_core::write_schemas(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        // `oath new` and no `oath apply`.
        let spec = json!({"exec": ["/bin/sleep", "900001"]}).as_object().unwrap().clone();
        cat.new_object(&"svc:t1".parse().unwrap(), spec).unwrap();
        // Applied, as a seeded or applied object is.
        let t2 = d.path().join("objects/svc/t2");
        fs::create_dir_all(&t2).unwrap();
        for f in ["desired.json", "applied.json"] {
            oath_core::write_json(&t2.join(f), &json!({"exec": ["/bin/sleep", "9"]})).unwrap();
        }
        let (wanted, unreadable) = wanted(d.path());
        assert!(unreadable.is_empty());
        assert_eq!(wanted.keys().collect::<Vec<_>>(), ["svc:t2"]);
    }

    #[test]
    fn a_run_its_restart_does_not_follow_stays_down() {
        let svc = |restart: &str| -> Svc {
            serde_json::from_value(json!({"exec": ["/bin/true"], "restart": restart})).unwrap()
        };
        let (ok, bad) = (Exit::Code(0), Exit::Signal(15));
        // on-failure: exit 0 is finished, and an unrelated converge leaves it
        // down; a failure restarts it.
        assert!(!starts_again(SvcRestart::OnFailure, ok));
        assert!(finished(&svc("on-failure"), true, Some(ok)));
        assert!(starts_again(SvcRestart::OnFailure, bad));
        assert!(!finished(&svc("on-failure"), false, Some(bad)));
        // A oneshot that started this boot stays down, however it ended.
        assert!(!starts_again(SvcRestart::Never, ok) && !starts_again(SvcRestart::Never, bad));
        assert!(finished(&svc("never"), true, None));
        assert!(finished(&svc("never"), true, Some(bad)));
        // A new boot, or an apply of enabled=false, clears `done`.
        assert!(!finished(&svc("on-failure"), false, Some(ok)));
        assert!(!finished(&svc("never"), false, Some(ok)));
        // A `restart` that would have started it again does.
        assert!(!finished(&svc("always"), true, Some(ok)));
        assert!(!finished(&svc("on-failure"), true, Some(bad)));
    }

    /// Lines a guard wrote, without their stamps.
    fn written(out: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(out)
            .lines()
            .map(|l| l.split_once(' ').map_or(l, |(_, rest)| rest).to_string())
            .collect()
    }

    #[test]
    fn a_line_that_repeats_is_counted() {
        let (mut g, mut out, now) = (Gate::default(), Vec::new(), Instant::now());
        for _ in 0..4 {
            g.line(b"Protocol error 2", "T1", now, &mut out);
        }
        g.line(b"other", "T2", now, &mut out);
        g.line(b"other", "T2", now, &mut out);
        assert_eq!(
            written(&out),
            ["Protocol error 2", "oath-init: repeated 3 more times: Protocol error 2", "other"]
        );
        // The window's end says a count still open, and a repeat goes on
        // being counted after it.
        out.clear();
        g.line(b"other", "T3", now + BURST_WINDOW, &mut out);
        assert_eq!(written(&out), ["oath-init: repeated 1 more times: other"]);
        // An exit says the rest; the next run's first line is its own.
        out.clear();
        g.close("T4", &mut out);
        g.line(b"other", "T5", now + BURST_WINDOW, &mut out);
        assert_eq!(written(&out), ["oath-init: repeated 1 more times: other", "other"]);
    }

    #[test]
    fn past_the_burst_lines_are_held_back_and_the_newest_kept() {
        let (mut g, mut out, now) = (Gate::default(), Vec::new(), Instant::now());
        for i in 0..BURST_LINES + 3000 {
            g.line(format!("flood-{i}").as_bytes(), "T1", now, &mut out);
        }
        g.line(b"last words", "T2", now, &mut out);
        let w = written(&out);
        assert_eq!(w.len(), BURST_LINES);
        assert_eq!(w.last().map(String::as_str), Some(format!("flood-{}", BURST_LINES - 1).as_str()));
        // The window's end writes the newest of them, then the count.
        out.clear();
        g.roll("T3", now + BURST_WINDOW, &mut out);
        let w = written(&out);
        assert_eq!(w[w.len() - 2], "last words");
        let note = w.last().unwrap();
        assert!(note.starts_with("oath-init: held back 3001 lines (") && note.contains("since T1"));
        let kept: usize = w[..w.len() - 1].iter().map(|l| l.len() + 1).sum();
        assert!(kept <= HELD_MOST && kept > HELD_MOST - 32, "the newest {kept} bytes are kept");
        assert!(note.ends_with(&format!("the last {} of them are above", w.len() - 1)));
        // The new window writes again.
        out.clear();
        g.line(b"after", "T4", now + BURST_WINDOW, &mut out);
        assert_eq!(written(&out), ["after"]);
    }

    #[test]
    fn an_exit_writes_what_the_guard_held() {
        let (mut g, mut out, now) = (Gate::default(), Vec::new(), Instant::now());
        g.line(&vec![b'x'; BURST_BYTES - 10], "T1", now, &mut out);
        g.line(b"too long for the window", "T1", now, &mut out);
        g.line(b"short", "T1", now, &mut out);
        assert_eq!(written(&out).len(), 1, "once it holds one back it holds the rest");
        out.clear();
        g.close("T2", &mut out);
        let w = written(&out);
        assert_eq!(w[..2], ["too long for the window", "short"]);
        assert!(w[2].starts_with("oath-init: held back 2 lines (1 KiB) since T1"));
    }

    #[test]
    fn a_run_with_no_newline_is_cut_into_lines() {
        let (mut g, now) = (Gate::default(), Instant::now());
        let mut partial = [vec![b'a'; LINE_MOST], vec![b'c'; LINE_MOST], vec![b'a'; 5]].concat();
        let w = written(&take_lines(&mut partial, false, &mut g, "T", now));
        assert_eq!(w.len(), 2);
        assert_eq!(partial.len(), 5, "the rest waits for its newline");
        partial.extend_from_slice(b"b\ntail");
        assert_eq!(written(&take_lines(&mut partial, false, &mut g, "T", now)), ["aaaaab"]);
        assert_eq!(written(&take_lines(&mut partial, true, &mut g, "T", now)), ["tail"]);
        assert!(partial.is_empty());
    }

    #[test]
    fn a_failed_exit_keeps_the_lines_before_it() {
        let d = tempfile::tempdir().unwrap();
        let (log, exits) = (d.path().join("a.log"), d.path().join("a.exit.log"));
        let line = |i: usize| format!("2026-11-22T17:02:13Z line {i:06}\n");
        // The log just rotated: the lines before the exit span both files.
        let old: String = (0..3000).map(line).collect();
        let new: String = (3000..3010).map(line).collect();
        fs::write(rotated(&log), old).unwrap();
        fs::write(&log, new).unwrap();
        for i in 0..EXITS_KEPT + 1 {
            keep_exit(&log, &exits, &format!("== T{i} svc:a exit Signal(6) after 9 s"));
        }
        let kept = fs::read_to_string(&exits).unwrap();
        let ends: Vec<&str> = kept.lines().filter(|l| l.starts_with("== ")).collect();
        assert_eq!(ends.len(), EXITS_KEPT, "the last {EXITS_KEPT} exits");
        assert!(ends[0].starts_with("== T1 ") && ends[EXITS_KEPT - 1].starts_with("== T3 "));
        let last: Vec<&str> = kept.rsplit("== T2").next().unwrap().lines().skip(1).collect();
        assert_eq!(last[last.len() - 2], "2026-11-22T17:02:13Z line 003009");
        assert!(last[0].starts_with("2026-11-22T17:02:13Z line "), "whole lines: {}", last[0]);
        let bytes: usize = last[..last.len() - 1].iter().map(|l| l.len() + 1).sum();
        assert!(bytes <= EXIT_TAIL && bytes > EXIT_TAIL - 64);
    }

    #[test]
    fn exit_logs_sit_beside_logs() {
        assert_eq!(exit_log_path("svc:rivulet"), Path::new("/oath/log/svc/rivulet.exit.log"));
        assert_eq!(exit_log_path("net:net0"), Path::new("/oath/log/net/net0.exit.log"));
    }

    #[test]
    fn exit_json() {
        assert_eq!(Exit::Code(3).json(), json!({"code": 3}));
        assert_eq!(Exit::Signal(9).json(), json!({"signal": 9}));
        assert!(Exit::Code(1).failed() && Exit::Signal(15).failed() && !Exit::Code(0).failed());
    }
}
