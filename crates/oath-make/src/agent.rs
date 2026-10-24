//! `cargo make check --agent`: the agent surface's acceptance test
//! (docs/specs/agent.md, step 2). A model that has never seen Oath gets
//! one MCP server, the machine's `oath mcp` over SSH, and a task from
//! `check/agent.toml`. The machine is then checked over SSH: what the
//! model says it did is not evidence.
//!
//! Each task gets a fresh QEMU of the output, or with `--target` runs on
//! an installed machine and is undone afterwards like a metal case.
//! Opt-in: it costs money and is not deterministic. The client reads
//! `ANTHROPIC_API_KEY` from the environment; nothing here writes it down.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::check::{self, Conn, Ctx, Step, Vm};
use crate::util::{utc_rfc3339, utc_stamp, write_pretty};

/// Clear of `cargo make check`'s ports, so both can run at once.
const FIRST_SSH_PORT: u16 = 2400;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentFile {
    /// The client's argv, with `{prompt}`, `{mcp_config}`, `{model}` and
    /// `{budget}` to fill in.
    client: Vec<String>,
    model: String,
    budget: String,
    /// Seconds the client may run, per task.
    timeout: u64,
    task: Vec<Task>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    name: String,
    prompt: String,
    #[serde(default)]
    setup: Vec<Step>,
    expect: Vec<Step>,
    /// The task allows an apply with `confirm`.
    #[serde(default)]
    confirm: bool,
    /// The task ends in a reboot.
    #[serde(default)]
    reboots: bool,
}

pub struct Opts {
    pub only: Vec<String>,
    pub jobs: usize,
    pub target: Option<String>,
    pub output: Option<String>,
    pub model: Option<String>,
}

/// One task's outcome, as `agent-check.json` records it.
struct Outcome {
    task: String,
    pass: bool,
    cost: Option<f64>,
    turns: Option<u64>,
    detail: String,
}

/// The run as a whole: the client and where its files go.
struct Run {
    file: AgentFile,
    model: String,
    /// Transcripts, `<task>.jsonl`.
    dir: PathBuf,
    /// The client's working directory: empty.
    cwd: PathBuf,
}

pub fn run_agent(root: &Path, out: &Path, opts: Opts) -> Result<i32> {
    if std::env::var("ANTHROPIC_API_KEY").map_or(true, |k| k.trim().is_empty()) {
        bail!(
            "cargo make check --agent runs a real model: it needs ANTHROPIC_API_KEY in the \
             environment (never in a file under build/)"
        );
    }
    let file = load(&root.join("check/agent.toml"))?;
    let tasks: Vec<Task> = if opts.only.is_empty() {
        file.task.clone()
    } else {
        let picked: Vec<Task> =
            file.task.iter().filter(|t| opts.only.contains(&t.name)).cloned().collect();
        if picked.is_empty() {
            bail!("no task named {:?} in check/agent.toml", opts.only);
        }
        picked
    };
    let model = opts.model.clone().unwrap_or_else(|| file.model.clone());
    match &opts.target {
        Some(target) => run_target(out, file, model, tasks, target),
        None => run_qemu(root, out, file, model, tasks, &opts),
    }
}

fn load(path: &Path) -> Result<AgentFile> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse(&text).with_context(|| format!("parse {}", path.display()))
}

fn parse(text: &str) -> Result<AgentFile> {
    let f: AgentFile = toml::from_str(text)?;
    if !f.client.iter().any(|a| a.contains("{prompt}"))
        || !f.client.iter().any(|a| a.contains("{mcp_config}"))
    {
        bail!("client must take {{prompt}} and {{mcp_config}}");
    }
    let mut seen = std::collections::HashSet::new();
    for t in &f.task {
        if !seen.insert(&t.name) {
            bail!("task `{}` appears twice", t.name);
        }
        if t.setup.iter().chain(&t.expect).any(|s| s.run.is_none() || s.reboot.is_some()) {
            bail!("task `{}`: setup and expect steps are `run` steps", t.name);
        }
        if t.expect.is_empty() {
            bail!("task `{}` checks nothing", t.name);
        }
    }
    Ok(f)
}

fn run_qemu(
    root: &Path,
    out: &Path,
    file: AgentFile,
    model: String,
    tasks: Vec<Task>,
    opts: &Opts,
) -> Result<i32> {
    let tools = crate::tools::load(root)?;
    let output = crate::output::pick(out, opts.output.as_deref(), false)?;
    let img = crate::qemu::load_image(&output.dir)?;
    let base = out.join("check");
    fs::create_dir_all(&base)?;
    let (key, key_pub) = check::harness_key(&base)?;
    // Transcripts and the report go with the output; disks, serial logs
    // and steps with the harness's own, under build/check/.
    let dir = output.dir.join("agent");
    let vms = base.join(format!("{}-agent", utc_stamp()));
    let cwd = vms.join("cwd");
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(&cwd)?;
    eprintln!(
        "check --agent: output {}, model {model}, {} tasks, {} at a time, transcripts in {}, \
         logs in {}",
        output.id,
        tasks.len(),
        opts.jobs,
        dir.display(),
        vms.display()
    );
    let ctx = Arc::new(Ctx { tools, img, key, key_pub, dir: vms });
    let run = Arc::new(Run { file, model, dir, cwd });
    let queue = Arc::new(Mutex::new(tasks.into_iter().collect::<VecDeque<_>>()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut workers = Vec::new();
    for w in 0..opts.jobs.max(1) {
        let (ctx, run, queue, results) = (ctx.clone(), run.clone(), queue.clone(), results.clone());
        workers.push(thread::spawn(move || loop {
            let Some(task) = queue.lock().unwrap().pop_front() else { break };
            let o = task_in_vm(&ctx, &run, &task, FIRST_SSH_PORT + w as u16);
            print_outcome(&o);
            results.lock().unwrap().push(o);
        }));
    }
    for w in workers {
        let _ = w.join();
    }
    let results = std::mem::take(&mut *results.lock().unwrap());
    let report = output.file("agent-check.json");
    finish(&run, results, &report, json!({ "output": output.id }))
}

fn task_in_vm(ctx: &Ctx, run: &Run, task: &Task, port: u16) -> Outcome {
    let dir = ctx.dir.join(&task.name);
    let conn = Conn::vm(&ctx.key, port);
    let mut log = String::new();
    let mut client = None;
    let result = (|| -> Result<(), String> {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut boot_no = 1;
        let mut vm = Vm::boot(ctx, &dir, port, boot_no, 0).map_err(|e| e.to_string())?;
        if !check::wait_ssh(&conn, check::BOOT_WAIT) {
            return Err("no SSH after boot".into());
        }
        let before = prepare(&conn, task, &mut log)?;
        client = Some(run_client(run, task, &conn, &dir));
        // A reboot ends QEMU (-no-reboot): boot the same disk again.
        if task.reboots && !vm.exited_within(check::REBOOT_WAIT) {
            return Err("the machine did not reboot".into());
        }
        if vm.exited() {
            boot_no += 1;
            vm = Vm::boot(ctx, &dir, port, boot_no, 0).map_err(|e| e.to_string())?;
            if !check::wait_ssh(&conn, check::BOOT_WAIT) {
                return Err("no SSH after the reboot".into());
            }
        }
        let judged = judge(&conn, task, &before, &mut log);
        drop(vm);
        judged
    })();
    let _ = fs::write(dir.join("steps.log"), &log);
    outcome(task, client, result)
}

fn run_target(
    out: &Path,
    file: AgentFile,
    model: String,
    tasks: Vec<Task>,
    target: &str,
) -> Result<i32> {
    let r = crate::install::parse_target(target)?;
    let pins = crate::install::known_hosts(out);
    if !pins.is_file() {
        bail!("no pinned host keys ({}): `cargo make install` writes them", pins.display());
    }
    let conn = Conn::target(&r, &pins);
    let dir = out.join("check").join(format!("{}-{}-agent", utc_stamp(), r.host));
    let cwd = dir.join("cwd");
    fs::create_dir_all(&cwd)?;
    eprintln!(
        "check --agent: {} tasks on {target}, model {model}, one at a time, in {}",
        tasks.len(),
        dir.display()
    );
    let run = Run { file, model, dir: dir.clone(), cwd };
    let mut results = Vec::new();
    for task in &tasks {
        let tdir = dir.join(&task.name);
        let _ = fs::create_dir_all(&tdir);
        let mut log = String::new();
        let start = check::generation(&conn);
        let mut client = None;
        let result = (|| -> Result<(), String> {
            start.clone()?;
            let d = check::ssh(&conn, "oath diff", Duration::from_secs(30));
            if d.text.trim() != "in-sync" {
                return Err(format!("not in-sync before the task: {}", d.text));
            }
            let before = prepare(&conn, task, &mut log)?;
            client = Some(run_client(&run, task, &conn, &tdir));
            if task.reboots && !check::rebooted(&conn, &before.boot_id, check::METAL_BOOT_WAIT) {
                return Err("the machine did not reboot".into());
            }
            judge(&conn, task, &before, &mut log)
        })();
        let left = start.and_then(|g| check::restore(&conn, g, &mut log));
        let _ = fs::write(tdir.join("steps.log"), &log);
        let o = outcome(task, client, result);
        print_outcome(&o);
        results.push(o);
        if let Err(why) = left {
            eprintln!("stopping: {target} is not as `{}` found it: {why}", task.name);
            break;
        }
    }
    let report = dir.join("agent-check.json");
    let all = results.len() == tasks.len();
    let rc = finish(&run, results, &report, json!({ "target": target }))?;
    Ok(if all { rc } else { 1 })
}

/// The machine before the client runs.
struct Before {
    /// Apply log entries so far: the task's own come after.
    log_len: usize,
    boot_id: String,
}

/// Run the task's setup and note where the log and boot stand.
fn prepare(conn: &Conn, task: &Task, log: &mut String) -> Result<Before, String> {
    log.push_str("# setup\n");
    for (i, step) in task.setup.iter().enumerate() {
        check::run_step(conn, i + 1, step, log)?;
    }
    Ok(Before { log_len: apply_log(conn)?.len(), boot_id: check::boot_id(conn)? })
}

/// After the client: a reboot if the task asked for one, the expect steps,
/// and the apply log.
fn judge(conn: &Conn, task: &Task, before: &Before, log: &mut String) -> Result<(), String> {
    if task.reboots && check::boot_id(conn)? == before.boot_id {
        return Err("the boot id did not change".into());
    }
    log.push_str("# expect\n");
    for (i, step) in task.expect.iter().enumerate() {
        check::run_step(conn, i + 1, step, log)?;
    }
    let entries = apply_log(conn)?;
    let new = entries.get(before.log_len..).unwrap_or_default();
    log.push_str(&format!("# apply log since setup\n{}\n", serde_json::to_string(new).unwrap()));
    for e in new {
        let actor = &e["actor"];
        // PID 1 undoing an apply nobody kept is not a change of the model's.
        if actor["tty"] == "probation" {
            continue;
        }
        if !actor["via"].as_str().is_some_and(|v| v.starts_with("mcp:")) {
            return Err(format!("a change that did not come through MCP: {e}"));
        }
        if e["confirm"] == json!(true) && !task.confirm {
            return Err(format!("the model passed confirm, which the task did not ask for: {e}"));
        }
    }
    Ok(())
}

fn apply_log(conn: &Conn) -> Result<Vec<Value>, String> {
    let r = check::ssh(conn, "oath --json log", Duration::from_secs(30));
    serde_json::from_str(r.text.trim()).map_err(|e| format!("oath --json log: {e}: {}", r.text))
}

/// What the client's transcript says about the run.
#[derive(Default)]
struct Client {
    cost: Option<f64>,
    turns: Option<u64>,
    /// Its final word, for the report: `success`, `error_max_budget_usd`, …
    end: String,
    /// Exit code, or why it has none.
    exit: String,
}

/// Run the client on the task's prompt with one MCP server: the machine's
/// `oath mcp` over SSH. The transcript goes to `<task>.jsonl`.
fn run_client(run: &Run, task: &Task, conn: &Conn, dir: &Path) -> Client {
    let mut ssh_args = vec!["-T".to_string()];
    ssh_args.extend(conn.args.iter().cloned());
    ssh_args.push("oath mcp".into());
    let config = json!({ "mcpServers": { "oath": { "command": "ssh", "args": ssh_args } } });
    let config_path = dir.join("mcp.json");
    let transcript = run.dir.join(format!("{}.jsonl", task.name));
    let fill = |a: &str| {
        a.replace("{prompt}", &task.prompt)
            .replace("{mcp_config}", &config_path.display().to_string())
            .replace("{model}", &run.model)
            .replace("{budget}", &run.file.budget)
    };
    let argv: Vec<String> = run.file.client.iter().map(|a| fill(a)).collect();
    let mut c = Client::default();
    let spawned = (|| -> Result<std::process::Child> {
        write_pretty(&config_path, &config)?;
        Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&run.cwd)
            .stdin(Stdio::null())
            .stdout(fs::File::create(&transcript)?)
            .stderr(fs::File::create(dir.join("client.err"))?)
            .spawn()
            .with_context(|| format!("run {}", argv[0]))
    })();
    let mut child = match spawned {
        Ok(ch) => ch,
        Err(e) => {
            c.exit = format!("{e:#}");
            return c;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(run.file.timeout);
    c.exit = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().map_or("signal".into(), |n| n.to_string()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(200)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break format!("killed after {} s", run.file.timeout);
            }
        }
    };
    let text = fs::read_to_string(&transcript).unwrap_or_default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["type"] == "result" {
            c.cost = v["total_cost_usd"].as_f64();
            c.turns = v["num_turns"].as_u64();
            c.end = v["subtype"].as_str().unwrap_or("").to_string();
            if v["is_error"] == json!(true) {
                c.end.push_str(&format!(": {}", v["result"].as_str().unwrap_or("")));
            }
        }
    }
    c
}

fn outcome(task: &Task, client: Option<Client>, result: Result<(), String>) -> Outcome {
    let c = client.unwrap_or_default();
    let mut detail = result.as_ref().err().cloned().unwrap_or_default();
    if result.is_err() && (c.exit != "0" || c.end != "success") && !c.exit.is_empty() {
        detail.push_str(&format!(" (client: exit {}, {})", c.exit, c.end));
    }
    Outcome { task: task.name.clone(), pass: result.is_ok(), cost: c.cost, turns: c.turns, detail }
}

fn print_outcome(o: &Outcome) {
    let cost = o.cost.map_or(String::new(), |c| format!(" ${c:.4}"));
    let turns = o.turns.map_or(String::new(), |t| format!(" {t} turns"));
    let detail = if o.detail.is_empty() { String::new() } else { format!(" — {}", o.detail) };
    eprintln!("{:5}  {}{cost}{turns}{detail}", if o.pass { "PASS" } else { "FAIL" }, o.task);
}

/// Write the report, print the totals, and return the exit code (this
/// run's tasks only). A run of some tasks replaces their entries and keeps
/// the others', as their transcripts stay.
fn finish(run: &Run, mut results: Vec<Outcome>, report: &Path, place: Value) -> Result<i32> {
    results.sort_by(|a, b| a.task.cmp(&b.task));
    let time = utc_rfc3339();
    let tasks: Vec<Value> = results
        .iter()
        .map(|o| {
            json!({
                "task": o.task,
                "model": run.model,
                "pass": o.pass,
                "cost": o.cost,
                "turns": o.turns,
                "detail": o.detail,
                "time": time,
            })
        })
        .collect();
    let before: Option<Value> =
        fs::read_to_string(report).ok().and_then(|t| serde_json::from_str(&t).ok());
    let mut doc = json!({ "time": time, "tasks": merge(before.as_ref(), tasks) });
    if let (Some(d), Some(p)) = (doc.as_object_mut(), place.as_object()) {
        d.extend(p.clone());
    }
    write_pretty(report, &doc)?;
    let passed = results.iter().filter(|o| o.pass).count();
    let cost: f64 = results.iter().filter_map(|o| o.cost).sum();
    println!();
    for o in &results {
        println!("{:5}  {}", if o.pass { "PASS" } else { "FAIL" }, o.task);
    }
    println!(
        "\n{passed} of {} tasks pass with {}, ${cost:.4} — {}",
        results.len(),
        run.model,
        report.display()
    );
    Ok(if passed == results.len() { 0 } else { 1 })
}

/// The entries of an earlier report for tasks this run did not run, then
/// this run's, by task name.
fn merge(before: Option<&Value>, now: Vec<Value>) -> Vec<Value> {
    let mut all: Vec<Value> = before
        .and_then(|b| b["tasks"].as_array())
        .into_iter()
        .flatten()
        .filter(|t| !now.iter().any(|n| n["task"] == t["task"]))
        .cloned()
        .collect();
    all.extend(now);
    all.sort_by(|a, b| a["task"].as_str().cmp(&b["task"].as_str()));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_run_replaces_only_its_tasks() {
        let before = json!({ "tasks": [
            { "task": "rename", "pass": true, "model": "haiku" },
            { "task": "diagnose", "pass": false, "model": "haiku" },
        ]});
        let now = vec![json!({ "task": "diagnose", "pass": true, "model": "sonnet" })];
        let all = merge(Some(&before), now);
        assert_eq!(
            all,
            [
                json!({ "task": "diagnose", "pass": true, "model": "sonnet" }),
                json!({ "task": "rename", "pass": true, "model": "haiku" }),
            ]
        );
        assert_eq!(merge(None, vec![json!({ "task": "a" })]).len(), 1);
    }

    #[test]
    fn repo_tasks_parse() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../check/agent.toml");
        let f = load(&path).unwrap();
        let names: Vec<&str> = f.task.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["rename", "service", "undo", "diagnose", "reboot"]);
        assert_eq!(f.model, "haiku");
        assert!(f.client.contains(&"--bare".to_string()));
        let reboot = f.task.iter().find(|t| t.name == "reboot").unwrap();
        assert!(reboot.confirm && reboot.reboots);
        assert!(f.task.iter().filter(|t| t.name != "reboot").all(|t| !t.confirm));
    }

    #[test]
    fn bad_files_are_refused() {
        let client = r#"client = ["c", "{prompt}", "{mcp_config}"]
model = "m"
budget = "1"
timeout = 1
"#;
        let ok = format!("{client}[[task]]\nname='a'\nprompt='p'\nexpect=[{{run='true'}}]\n");
        assert!(parse(&ok).is_ok());
        assert!(parse(&ok.replace("{mcp_config}", "x")).is_err(), "no server");
        assert!(parse(&format!("{ok}[[task]]\nname='a'\nprompt='p'\nexpect=[{{run='true'}}]\n"))
            .is_err());
        assert!(parse(&ok.replace("run='true'", "reboot='force'")).is_err());
        assert!(parse(&format!("{client}[[task]]\nname='a'\nprompt='p'\nexpect=[]\n")).is_err());
        assert!(parse(&ok.replace("prompt='p'", "prompt='p'\nbogus=1")).is_err());
    }
}
