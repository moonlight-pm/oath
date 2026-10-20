//! `cargo make check`: boot the built image in QEMU and run `check/cases.toml`
//! over SSH. Every case gets a fresh VM from a build output's `oath.qcow2`
//! (the latest, or `--output <id>`); cases run
//! in parallel. A case with `gap` is a known failure scheduled for a phase:
//! it may fail without failing the run, and if it starts passing the run
//! fails so the gap gets removed. A clean run writes `check-pass` into the
//! output, which `cargo make install` requires.
//!
//! `--target user@host` runs the cases marked `metal` against a real
//! machine instead, one at a time. After each case the harness undoes back
//! to the generation the case started from and requires `oath diff` to be
//! in-sync; a machine left otherwise stops the run.

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::install::Remote;
use crate::qemu::{self, Image, SerialMode, VmSpec};
use crate::tools::Tools;
use crate::util::{sha256_file, utc_rfc3339, utc_stamp, write_pretty};

/// The fetch canary. Cases make `pkg:fetchme` with `url`
/// http://10.0.2.2:18765/fetchme, which QEMU's user net sends here, or
/// pin it and list http://10.0.2.2:18765/origin in `host:local.origins`:
/// an origin with `INDEX.md` and `pkg/fetchme/<hash>.tar` (apps/fetchme).
const FETCH_PORT: u16 = 18765;
const FIRST_SSH_PORT: u16 = 2300;
pub(crate) const BOOT_WAIT: Duration = Duration::from_secs(90);
pub(crate) const REBOOT_WAIT: Duration = Duration::from_secs(45);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    case: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    /// What the case shows, one line.
    #[serde(default)]
    pub about: String,
    /// Known failure: the phase and finding that will fix it.
    #[serde(default)]
    pub gap: Option<String>,
    /// Also runs on a real machine (`check --target`): it expects nothing
    /// QEMU-specific and leaves no change that `oath undo` cannot take back.
    #[serde(default)]
    pub metal: bool,
    /// Runs only on an image of this seed profile (`base`, `desk`); by
    /// default on every profile.
    #[serde(default)]
    pub profile: Option<String>,
    /// `false`: a metal case that QEMU cannot run (no GL for Sola's shell).
    #[serde(default = "yes")]
    pub qemu: bool,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Shell command run over SSH as `home`.
    #[serde(default)]
    pub run: Option<String>,
    /// The command must exit non-zero (default: must exit 0).
    #[serde(default)]
    pub fails: bool,
    /// Substrings that must appear in stdout + stderr.
    #[serde(default)]
    pub expect: Vec<String>,
    /// Substrings that must not appear.
    #[serde(default)]
    pub reject: Vec<String>,
    /// `catalog` (power=reboot + apply --confirm), `force` (`reboot -f`),
    /// or `signal` (plain `reboot`, which asks PID 1). Waits for QEMU to
    /// exit, boots the same disk again and waits for SSH.
    #[serde(default)]
    pub reboot: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    60
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// Known gap, still failing.
    Gap,
    /// Known gap that now passes: remove the gap.
    Fixed,
}

impl Verdict {
    fn word(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Gap => "GAP",
            Self::Fixed => "FIXED",
        }
    }

    fn ok(self) -> bool {
        matches!(self, Self::Pass | Self::Gap)
    }
}

pub fn verdict(gap: bool, passed: bool) -> Verdict {
    match (gap, passed) {
        (false, true) => Verdict::Pass,
        (false, false) => Verdict::Fail,
        (true, false) => Verdict::Gap,
        (true, true) => Verdict::Fixed,
    }
}

pub struct Opts {
    pub only: Vec<String>,
    pub jobs: usize,
    /// `user@host[:port]`: run the `metal` cases on this machine, not QEMU.
    pub target: Option<String>,
    /// The build output to boot (default: the latest).
    pub output: Option<String>,
}

pub(crate) struct Ctx {
    pub(crate) tools: Tools,
    pub(crate) img: Image,
    pub(crate) key: PathBuf,
    pub(crate) key_pub: PathBuf,
    pub(crate) dir: PathBuf,
}

struct Outcome {
    name: String,
    about: String,
    gap: Option<String>,
    verdict: Verdict,
    detail: String,
}

pub fn run_check(root: &Path, out: &Path, opts: Opts) -> Result<i32> {
    let cases = load_cases(&root.join("check/cases.toml"))?;
    let cases: Vec<Case> = if opts.only.is_empty() {
        cases
    } else {
        let picked: Vec<Case> =
            cases.into_iter().filter(|c| opts.only.iter().any(|o| o == &c.name)).collect();
        if picked.is_empty() {
            bail!("no case named {:?}", opts.only);
        }
        picked
    };
    if let Some(target) = &opts.target {
        return run_target(out, cases, target);
    }
    let tools = crate::tools::load(root)?;
    let output = crate::output::pick(out, opts.output.as_deref(), false)?;
    let profile = output_profile(&output.dir)?;
    let cases = for_profile(cases.into_iter().filter(|c| c.qemu).collect(), &profile)?;
    let img = qemu::load_image(&output.dir)?;
    let base = out.join("check");
    fs::create_dir_all(&base)?;
    let (key, key_pub) = harness_key(&base)?;
    let dir = base.join(utc_stamp());
    fs::create_dir_all(&dir)?;
    let fetchme = fs::read(root.join("apps/fetchme/bin/fetchme")).context("apps/fetchme")?;
    serve_fetchme(fetchme, fetchme_origin(root)?)?;

    eprintln!(
        "check: output {} ({profile}), {} cases, {} at a time, logs in {}",
        output.id,
        cases.len(),
        opts.jobs,
        dir.display()
    );
    let ctx = Arc::new(Ctx { tools, img, key, key_pub, dir: dir.clone() });
    let queue = Arc::new(Mutex::new(cases.into_iter().collect::<VecDeque<_>>()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut workers = Vec::new();
    for w in 0..opts.jobs.max(1) {
        let (ctx, queue, results) = (ctx.clone(), queue.clone(), results.clone());
        workers.push(thread::spawn(move || loop {
            let Some(case) = queue.lock().unwrap().pop_front() else { break };
            let port = FIRST_SSH_PORT + w as u16;
            let outcome = run_case(&ctx, &case, port);
            eprintln!(
                "{:5}  {}{}",
                outcome.verdict.word(),
                outcome.name,
                if outcome.detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", outcome.detail)
                }
            );
            results.lock().unwrap().push(outcome);
        }));
    }
    for w in workers {
        let _ = w.join();
    }
    let mut results = std::mem::take(&mut *results.lock().unwrap());
    results.sort_by(|a, b| a.name.cmp(&b.name));
    let all_ok = summarize(&results, &dir)?;
    let stamp = output.file("check-pass");
    let _ = fs::remove_file(&stamp);
    if all_ok && opts.only.is_empty() {
        write_pretty(&stamp, &image_stamp(&ctx.img)?)?;
        println!("image checked: {}", stamp.display());
    }
    Ok(if all_ok { 0 } else { 1 })
}

/// Write `report.json`, print one line per case and the totals.
fn summarize(results: &[Outcome], dir: &Path) -> Result<bool> {
    let report: Vec<_> = results
        .iter()
        .map(|r| {
            json!({
                "case": r.name,
                "about": r.about,
                "verdict": r.verdict.word(),
                "gap": r.gap,
                "detail": r.detail,
            })
        })
        .collect();
    write_pretty(&dir.join("report.json"), &json!({ "time": utc_rfc3339(), "cases": report }))?;

    let count = |v: Verdict| results.iter().filter(|r| r.verdict == v).count();
    println!();
    for r in results {
        let note = match (&r.gap, r.verdict) {
            (Some(g), Verdict::Gap) => format!("  (known gap: {g})"),
            (Some(g), Verdict::Fixed) => format!("  (passes now: remove gap \"{g}\")"),
            _ => String::new(),
        };
        println!("{:5}  {}{note}", r.verdict.word(), r.name);
    }
    println!(
        "\n{} pass, {} fail, {} known gaps, {} fixed gaps — {}",
        count(Verdict::Pass),
        count(Verdict::Fail),
        count(Verdict::Gap),
        count(Verdict::Fixed),
        dir.display()
    );
    Ok(results.iter().all(|r| r.verdict.ok()))
}

/// What a clean check vouches for. `install` compares this with what it ships.
pub fn image_stamp(img: &Image) -> Result<serde_json::Value> {
    Ok(json!({
        "kernel": sha256_file(&img.kernel)?,
        "initrd": sha256_file(&img.initrd)?,
        "image": sha256_file(&img.backing)?,
    }))
}

/// Refuse to install an output that `cargo make check` has not passed.
pub fn require_checked(output: &Path) -> Result<()> {
    let stamp = output.join("check-pass");
    let Ok(text) = fs::read_to_string(&stamp) else {
        bail!("this image has not passed `cargo make check` (no {})", stamp.display());
    };
    let want: serde_json::Value = serde_json::from_str(&text)?;
    let have = image_stamp(&qemu::load_image(output)?)?;
    for k in ["kernel", "initrd", "image"] {
        if want[k] != have[k] {
            bail!("the {k} changed since the last `cargo make check`; run it again");
        }
    }
    Ok(())
}

fn yes() -> bool {
    true
}

/// The seed profile a build output was packed from (its manifest).
fn output_profile(dir: &Path) -> Result<String> {
    let m: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("manifest.json")).context("the output's manifest.json")?,
    )?;
    Ok(m["profile"].as_str().unwrap_or(oath_core::DEFAULT_PROFILE).to_string())
}

/// The cases for `profile`: those that name no profile, or this one.
fn for_profile(cases: Vec<Case>, profile: &str) -> Result<Vec<Case>> {
    let cases: Vec<Case> =
        cases.into_iter().filter(|c| c.profile.as_deref().is_none_or(|p| p == profile)).collect();
    if cases.is_empty() {
        bail!("none of the chosen cases runs on profile {profile}");
    }
    Ok(cases)
}

/// A machine's profile, from its catalog: the largest seed profile whose
/// objects it all has (an overlay only adds). `base` when none fits.
fn machine_profile(conn: &Conn) -> Result<String> {
    let ls = ssh(conn, "oath ls", Duration::from_secs(30));
    if ls.code != 0 {
        bail!("`oath ls` failed (exit {}): {}", ls.code, ls.text);
    }
    let have: BTreeSet<&str> = ls.text.lines().map(str::trim).collect();
    let mut best = (0, oath_core::DEFAULT_PROFILE.to_string());
    for (name, _) in oath_core::PROFILES {
        let ids = oath_core::Profile::load(name).map_err(|e| anyhow::anyhow!("{e}"))?.ids();
        if ids.len() > best.0 && ids.iter().all(|id| have.contains(id.as_str())) {
            best = (ids.len(), name.to_string());
        }
    }
    Ok(best.1)
}

pub fn load_cases(path: &Path) -> Result<Vec<Case>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_cases(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn parse_cases(text: &str) -> Result<Vec<Case>> {
    let file: CaseFile = toml::from_str(text)?;
    let mut seen = std::collections::HashSet::new();
    for c in &file.case {
        if !seen.insert(c.name.clone()) {
            bail!("case `{}` appears twice", c.name);
        }
        for (i, s) in c.steps.iter().enumerate() {
            match (&s.run, &s.reboot) {
                (Some(_), None) | (None, Some(_)) => {}
                _ => bail!("case `{}` step {}: give exactly one of run or reboot", c.name, i + 1),
            }
            if let Some(r) = &s.reboot {
                if !matches!(r.as_str(), "catalog" | "force" | "signal") {
                    bail!("case `{}` step {}: reboot is catalog, force or signal", c.name, i + 1);
                }
            }
        }
    }
    Ok(file.case)
}

pub(crate) fn harness_key(base: &Path) -> Result<(PathBuf, PathBuf)> {
    let key = base.join("key");
    let key_pub = base.join("key.pub");
    if !key.is_file() || !key_pub.is_file() {
        let _ = fs::remove_file(&key);
        let _ = fs::remove_file(&key_pub);
        let st = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "oath-check", "-f"])
            .arg(&key)
            .stdin(Stdio::null())
            .status()
            .context("ssh-keygen")?;
        if !st.success() {
            bail!("ssh-keygen failed");
        }
    }
    Ok((key, key_pub))
}

/// The canary's origin: path to body, `INDEX.md` and the pack's tar.
fn fetchme_origin(root: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let tree = root.join("apps/fetchme");
    let pin = oath_core::hash_tree(&tree).map_err(|e| anyhow::anyhow!("apps/fetchme: {e}"))?;
    let out = Command::new("tar").arg("-C").arg(&tree).args(["-cf", "-", "."]).output()?;
    if !out.status.success() {
        bail!("tar apps/fetchme failed");
    }
    let index = format!(
        "# Oath pack origin\n\n| name | hash | licence | plan | source |\n\
         |------|------|---------|------|--------|\n| `fetchme` | `{pin}` | MIT | | |\n"
    );
    Ok(vec![
        ("/origin/INDEX.md".into(), index.into_bytes()),
        (format!("/origin/pkg/fetchme/{pin}.tar"), out.stdout),
    ])
}

/// HTTP for the fetch canary. Thread lives for the rest of the process.
fn serve_fetchme(body: Vec<u8>, origin: Vec<(String, Vec<u8>)>) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", FETCH_PORT))
        .with_context(|| format!("127.0.0.1:{FETCH_PORT} is busy (the fetch canary needs it)"))?;
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut conn = conn;
            let mut buf = [0u8; 2048];
            let n = conn.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("");
            let found = if path == "/fetchme" {
                Some(&body)
            } else {
                origin.iter().find(|(p, _)| p == path).map(|(_, b)| b)
            };
            let resp = if let Some(body) = found {
                let mut r = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                r.extend_from_slice(body);
                r
            } else {
                b"HTTP/1.0 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
            };
            let _ = conn.write_all(&resp);
        }
    });
    Ok(())
}

pub(crate) struct Vm {
    child: Child,
}

impl Vm {
    pub(crate) fn boot(ctx: &Ctx, dir: &Path, port: u16, boot_no: u32) -> Result<Self> {
        let overlay = qemu::overlay_disk(&ctx.tools, dir, &ctx.img.backing)?;
        let serial_log = dir.join(format!("serial-{boot_no}.log"));
        let qemu_log = dir.join(format!("qemu-{boot_no}.log"));
        let args = qemu::qemu_args(
            &ctx.tools,
            &ctx.img,
            &VmSpec {
                overlay: &overlay,
                serial_log: &serial_log,
                qemu_log: &qemu_log,
                serial: SerialMode::File,
                inject_authorized: Some(&ctx.key_pub),
                window: false,
                ssh_port: port,
                mem_mb: 1024,
            },
        );
        fs::write(dir.join("qemu.cmd"), args.join(" ") + "\n")?;
        let child = qemu::qemu_command(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("qemu")?;
        Ok(Self { child })
    }

    pub(crate) fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub(crate) fn exited_within(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return true;
            }
            thread::sleep(Duration::from_millis(200));
        }
        false
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        self.kill();
    }
}

pub(crate) struct Shell {
    pub(crate) code: i32,
    pub(crate) text: String,
}

/// How to reach a machine: the ssh(1) arguments that go before the command.
pub(crate) struct Conn {
    pub(crate) args: Vec<String>,
}

impl Conn {
    /// A check VM: the harness key, any host key (every VM makes a new one).
    pub(crate) fn vm(key: &Path, port: u16) -> Self {
        let mut args: Vec<String> = vec!["-i".into(), key.display().to_string()];
        args.extend(
            [
                "-p",
                &port.to_string(),
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "IdentitiesOnly=yes",
            ]
            .map(String::from),
        );
        Self::finish(args, "home@127.0.0.1")
    }

    /// A real machine: the operator's keys, and only the host key `install`
    /// pinned for it.
    pub(crate) fn target(r: &Remote, pins: &Path) -> Self {
        let args = [
            "-p".to_string(),
            r.port.to_string(),
            "-o".into(),
            "StrictHostKeyChecking=yes".into(),
            "-o".into(),
            format!("UserKnownHostsFile={}", pins.display()),
        ]
        .to_vec();
        Self::finish(args, &format!("{}@{}", r.user, r.host))
    }

    fn finish(mut args: Vec<String>, dest: &str) -> Self {
        args.extend(
            [
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-o",
                "LogLevel=ERROR",
                dest,
                "--",
            ]
            .map(String::from),
        );
        Self { args }
    }
}

pub(crate) fn ssh(conn: &Conn, cmd: &str, timeout: Duration) -> Shell {
    let mut c = Command::new("ssh");
    // Steps are sh scripts; home's login shell (zsh) only starts sh.
    c.args(&conn.args).arg(format!("/bin/sh -c {}", crate::install::sh_quote(cmd)));
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let Ok(mut child) = c.spawn() else {
        return Shell { code: -1, text: "could not run ssh".into() };
    };
    // One reader per stream: a full stderr pipe must never block stdout.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(-1),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break -2;
            }
        }
    };
    let mut raw = out.join().unwrap_or_default();
    raw.extend(err.join().unwrap_or_default());
    let text = String::from_utf8_lossy(&raw)
        .lines()
        .filter(|l| !l.starts_with("oath-tel "))
        .collect::<Vec<_>>()
        .join("\n");
    Shell { code, text }
}

pub(crate) fn wait_ssh(conn: &Conn, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if ssh(conn, "test -f /oath/INDEX.md", Duration::from_secs(10)).code == 0 {
            return true;
        }
        thread::sleep(Duration::from_secs(1));
    }
    false
}

/// The command a `reboot` step sends.
fn reboot_cmd(how: &str) -> &'static str {
    match how {
        "catalog" => {
            "sudo oath set host:local power=reboot && sudo oath apply --confirm host:local"
        }
        "force" => "sync; sudo reboot -f",
        _ => "sudo reboot",
    }
}

/// Run one `run` step and judge its output.
pub(crate) fn run_step(conn: &Conn, n: usize, step: &Step, log: &mut String) -> Result<(), String> {
    let cmd = step.run.as_deref().unwrap_or("");
    let r = ssh(conn, cmd, Duration::from_secs(step.timeout));
    log.push_str(&format!("## step {n}\n$ {cmd}\n{}\n[exit {}]\n", r.text, r.code));
    if r.code == 255 || r.code < 0 {
        return Err(format!("step {n}: lost SSH (`{cmd}`)"));
    }
    if step.fails && r.code == 0 {
        return Err(format!("step {n}: expected failure, got exit 0 (`{cmd}`)"));
    }
    if !step.fails && r.code != 0 {
        return Err(format!("step {n}: exit {} (`{cmd}`)", r.code));
    }
    for want in &step.expect {
        if !r.text.contains(want.as_str()) {
            return Err(format!("step {n}: missing {want:?} (`{cmd}`)"));
        }
    }
    for bad in &step.reject {
        if r.text.contains(bad.as_str()) {
            return Err(format!("step {n}: unexpected {bad:?} (`{cmd}`)"));
        }
    }
    Ok(())
}

fn run_case(ctx: &Ctx, case: &Case, port: u16) -> Outcome {
    let dir = ctx.dir.join(&case.name);
    let conn = Conn::vm(&ctx.key, port);
    let mut log = String::new();
    let result = (|| -> Result<(), String> {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut boot_no = 1;
        let mut vm = Vm::boot(ctx, &dir, port, boot_no).map_err(|e| e.to_string())?;
        if !wait_ssh(&conn, BOOT_WAIT) {
            return Err("no SSH after boot".into());
        }
        for (i, step) in case.steps.iter().enumerate() {
            let n = i + 1;
            let Some(how) = &step.reboot else {
                run_step(&conn, n, step, &mut log)?;
                continue;
            };
            let cmd = reboot_cmd(how);
            log.push_str(&format!("## step {n}: reboot ({how})\n$ {cmd}\n"));
            let r = ssh(&conn, cmd, Duration::from_secs(20));
            log.push_str(&format!("{}\n[exit {}]\n", r.text, r.code));
            if !vm.exited_within(REBOOT_WAIT) {
                return Err(format!("step {n}: the VM did not reboot ({how})"));
            }
            boot_no += 1;
            vm = Vm::boot(ctx, &dir, port, boot_no).map_err(|e| e.to_string())?;
            if !wait_ssh(&conn, BOOT_WAIT) {
                return Err(format!("step {n}: no SSH after reboot"));
            }
        }
        Ok(())
    })();
    let _ = fs::write(dir.join("steps.log"), &log);
    outcome(case, result)
}

fn outcome(case: &Case, result: Result<(), String>) -> Outcome {
    let passed = result.is_ok();
    Outcome {
        name: case.name.clone(),
        about: case.about.clone(),
        gap: case.gap.clone(),
        verdict: verdict(case.gap.is_some(), passed),
        detail: result.err().unwrap_or_default(),
    }
}

// ---- real machines (`--target`) ----------------------------------------

/// A real machine is slower to come back than a VM (firmware, POST).
pub(crate) const METAL_BOOT_WAIT: Duration = Duration::from_secs(300);

fn run_target(out: &Path, cases: Vec<Case>, target: &str) -> Result<i32> {
    let r = crate::install::parse_target(target)?;
    let pins = crate::install::known_hosts(out);
    if !pins.is_file() {
        bail!(
            "no pinned host keys ({}): `cargo make install` writes the key of each machine it installs",
            pins.display()
        );
    }
    let conn = Conn::target(&r, &pins);
    let diff = ssh(&conn, "oath diff", Duration::from_secs(30));
    if diff.code != 0 {
        bail!("`oath diff` on {target} failed (exit {}): {}", diff.code, diff.text);
    }
    if diff.text.trim() != "in-sync" {
        bail!(
            "{target} has pending changes; check runs only on an in-sync machine:\n{}",
            diff.text
        );
    }
    let cases: Vec<Case> = cases.into_iter().filter(|c| c.metal).collect();
    if cases.is_empty() {
        bail!("none of the chosen cases is marked `metal`");
    }
    let profile = machine_profile(&conn)?;
    let cases = for_profile(cases, &profile)?;
    let dir = out.join("check").join(format!("{}-{}", utc_stamp(), r.host));
    fs::create_dir_all(&dir)?;
    eprintln!(
        "check: {} cases on {target} ({profile}), one at a time, logs in {}",
        cases.len(),
        dir.display()
    );
    let mut results = Vec::new();
    for case in &cases {
        let (o, left) = run_target_case(&conn, case, &dir);
        eprintln!(
            "{:5}  {}{}",
            o.verdict.word(),
            o.name,
            if o.detail.is_empty() { String::new() } else { format!(" — {}", o.detail) }
        );
        results.push(o);
        if let Err(why) = left {
            eprintln!("stopping: {target} is not as `{}` found it: {why}", case.name);
            break;
        }
    }
    let all_ok = summarize(&results, &dir)? && results.len() == cases.len();
    Ok(if all_ok { 0 } else { 1 })
}

/// Run a case, then put the machine back. The second result says whether
/// the machine is as the case found it.
fn run_target_case(conn: &Conn, case: &Case, dir: &Path) -> (Outcome, Result<(), String>) {
    let dir = dir.join(&case.name);
    let _ = fs::create_dir_all(&dir);
    let mut log = String::new();
    let start = generation(conn);
    let result = (|| -> Result<(), String> {
        start.clone()?;
        for (i, step) in case.steps.iter().enumerate() {
            let n = i + 1;
            let Some(how) = &step.reboot else {
                run_step(conn, n, step, &mut log)?;
                continue;
            };
            let before = boot_id(conn)?;
            let cmd = reboot_cmd(how);
            log.push_str(&format!("## step {n}: reboot ({how})\n$ {cmd}\n"));
            let r = ssh(conn, cmd, Duration::from_secs(20));
            log.push_str(&format!("{}\n[exit {}]\n", r.text, r.code));
            if !rebooted(conn, &before, METAL_BOOT_WAIT) {
                return Err(format!(
                    "step {n}: no new boot within {}s ({how})",
                    METAL_BOOT_WAIT.as_secs()
                ));
            }
        }
        Ok(())
    })();
    let left = start.and_then(|g| restore(conn, g, &mut log));
    let _ = fs::write(dir.join("steps.log"), &log);
    (outcome(case, result), left)
}

/// `snap:current`'s generation.
pub(crate) fn generation(conn: &Conn) -> Result<u64, String> {
    let r = ssh(conn, "oath --json get snap:current", Duration::from_secs(30));
    let v: serde_json::Value = serde_json::from_str(r.text.trim())
        .map_err(|e| format!("oath get snap:current: {e}: {}", r.text))?;
    v["desired"]["generation"].as_u64().ok_or_else(|| format!("no generation in {}", r.text))
}

/// Undo back to generation `start`, then require nothing pending.
pub(crate) fn restore(conn: &Conn, start: u64, log: &mut String) -> Result<(), String> {
    for _ in 0..20 {
        let g = generation(conn)?;
        if g == start {
            break;
        }
        let r = ssh(conn, "sudo oath undo", Duration::from_secs(120));
        log.push_str(&format!(
            "## restore (generation {g} -> {start})\n$ sudo oath undo\n{}\n[exit {}]\n",
            r.text, r.code
        ));
        if r.code != 0 {
            return Err(format!("undo at generation {g} failed: {}", r.text));
        }
    }
    let g = generation(conn)?;
    if g != start {
        return Err(format!("generation is {g}, the case started at {start}"));
    }
    let d = ssh(conn, "oath diff", Duration::from_secs(30));
    log.push_str(&format!("## restore: oath diff\n{}\n", d.text));
    if d.code != 0 || d.text.trim() != "in-sync" {
        return Err(format!("pending changes left: {}", d.text));
    }
    Ok(())
}

pub(crate) fn boot_id(conn: &Conn) -> Result<String, String> {
    let r = ssh(conn, "cat /proc/sys/kernel/random/boot_id", Duration::from_secs(15));
    if r.code != 0 {
        return Err(format!("read boot_id: {}", r.text));
    }
    Ok(r.text.trim().to_string())
}

/// Wait until the machine answers with a different boot id and a catalog.
pub(crate) fn rebooted(conn: &Conn, before: &str, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let r = ssh(
            conn,
            "test -f /oath/INDEX.md && cat /proc/sys/kernel/random/boot_id",
            Duration::from_secs(10),
        );
        if r.code == 0 && !r.text.trim().is_empty() && r.text.trim() != before {
            return true;
        }
        thread::sleep(Duration::from_secs(3));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_cases_parse() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../check/cases.toml");
        let cases = load_cases(&root).unwrap();
        assert!(cases.len() >= 5);
        assert!(cases.iter().any(|c| c.gap.is_none()));
    }

    #[test]
    fn metal_cases_expect_nothing_from_qemu() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../check/cases.toml");
        let cases = load_cases(&root).unwrap();
        assert!(cases.iter().any(|c| c.metal));
        for c in cases.iter().filter(|c| c.metal) {
            for s in &c.steps {
                let text = format!("{:?} {:?} {:?}", s.run, s.expect, s.reject);
                for qemu in ["10.0.2.", "fetchme", "/oath/objects/"] {
                    assert!(!text.contains(qemu), "metal case `{}` uses {qemu:?}", c.name);
                }
                // A machine is at whatever generation it is at.
                for (i, _) in text.match_indices("generation") {
                    let rest = text[i + "generation".len()..].trim_start_matches([':', ' ']);
                    assert!(
                        !rest.starts_with(|ch: char| ch.is_ascii_digit()),
                        "metal case `{}` names a generation number",
                        c.name
                    );
                }
            }
        }
    }

    #[test]
    fn step_needs_run_or_reboot() {
        assert!(parse_cases("[[case]]\nname='a'\nsteps=[{}]").is_err());
        assert!(parse_cases("[[case]]\nname='a'\nsteps=[{run='x', reboot='force'}]").is_err());
        assert!(parse_cases("[[case]]\nname='a'\nsteps=[{reboot='soon'}]").is_err());
        assert!(parse_cases(
            "[[case]]\nname='a'\nsteps=[{run='true'}]\n[[case]]\nname='a'\nsteps=[{run='true'}]"
        )
        .is_err());
        assert!(parse_cases("[[case]]\nname='a'\nsteps=[{run='true', bogus=1}]").is_err());
        let c = parse_cases("[[case]]\nname='a'\nsteps=[{run='true'},{reboot='force'}]").unwrap();
        assert_eq!(c[0].steps[0].timeout, 60);
    }

    #[test]
    fn gaps_flip_when_fixed() {
        assert_eq!(verdict(false, true), Verdict::Pass);
        assert_eq!(verdict(false, false), Verdict::Fail);
        assert_eq!(verdict(true, false), Verdict::Gap);
        assert_eq!(verdict(true, true), Verdict::Fixed);
        assert!(Verdict::Gap.ok() && !Verdict::Fixed.ok() && !Verdict::Fail.ok());
    }
}
