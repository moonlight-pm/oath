use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use oath_core::{
    apply_overlay, build_file, build_pinned, file_plan, fmt_plan, lint_bytes, lint_path,
    parse_plan, seed_profile, tel, Actor, BuildReport, Catalog, Error, ObjectId, DEFAULT_PROFILE,
    DEFAULT_ROOT, EXIT_CONFIRM, KIND_CELL, KIND_PKG, KIND_PLAN, KIND_SVC,
};
use serde_json::{json, Map, Value};

mod add;
mod install;
mod live;
mod login;
mod mcp;
mod audit;
mod overlay;
mod publish;
mod reap;
mod sleep;
mod update;
mod wake;

#[derive(Parser)]
#[command(name = "oath", about = "Oath catalog — the only admin surface.")]
struct Cli {
    /// Catalog root (appliance: /oath)
    #[arg(long, env = "OATH_ROOT", default_value = DEFAULT_ROOT, global = true)]
    root: PathBuf,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    confirm: bool,
    /// The tool running this command for the user (`mcp:<client>`), kept
    /// in the apply log as `actor.via`.
    #[arg(long, global = true, hide = true)]
    via: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Ls {
        #[arg(long)]
        kind: Option<String>,
        /// What the origins hold (R81), not what this machine has: each
        /// package's version and what it is, `*` on those here.
        #[arg(long, conflicts_with = "kind")]
        origin: bool,
        /// A kind, as --kind takes it (`oath ls svc`). With --origin: only
        /// packages with this in their name, category or description.
        #[arg(value_name = "KIND|WORD", conflicts_with = "kind")]
        word: Option<String>,
    },
    Schema {
        kind: Option<String>,
    },
    Get {
        id: String,
        #[arg(long)]
        actual: bool,
        #[arg(long)]
        desired: bool,
    },
    Set {
        id: String,
        #[arg(long = "from-json")]
        from_json: Option<String>,
        #[arg(long = "from-file")]
        from_file: Option<PathBuf>,
        #[arg(value_name = "K=V")]
        fields: Vec<String>,
    },
    /// Create a svc, pkg or cell. Nothing runs until `oath apply`.
    /// `oath new pkg:<name>` with no fields takes the package by name (R81):
    /// at its newest signed head, with what it needs that this machine lacks.
    New {
        id: String,
        #[arg(long = "from-json")]
        from_json: Option<String>,
        #[arg(value_name = "K=V")]
        fields: Vec<String>,
    },
    /// Install packages by name (R81): each at its newest signed head, with
    /// what it needs that this machine lacks, made and applied at once.
    /// `oath new pkg:<name>` with no fields makes them without applying.
    Add {
        /// `<name>` or `pkg:<name>`.
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Mark a svc, pkg or cell for removal; `oath apply` stops or unlinks it.
    /// One no apply has taken is removed at once.
    Rm {
        id: String,
    },
    /// Write a canonical plan.plan to stdout (or -w in place).
    Fmt {
        path: PathBuf,
        #[arg(short = 'w')]
        write: bool,
    },
    /// Put a tree built elsewhere (a directory, or a tar of one) into the
    /// store as a realization of `pkg:<name>` or `cell:<name>`, and print its
    /// hash. Changes no object: `oath set <id> hash=…` and `oath apply` do.
    Store {
        id: String,
        src: PathBuf,
    },
    /// Isolated plan build. Does not link /bin.
    Build {
        id: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        /// Make no `pkg:<name>` object for the product: it is only a
        /// build input to other plans.
        #[arg(long, requires = "id")]
        no_pkg: bool,
    },
    Diff {
        id: Option<String>,
    },
    Apply {
        ids: Vec<String>,
        /// Seconds a change that can cut off SSH stays on probation before
        /// PID 1 undoes it (unless `oath keep`).
        #[arg(long, default_value_t = oath_core::probation::DEFAULT_SECS)]
        probation: u64,
        /// A running service with a `seat` (the desk's, the console's) keeps
        /// its program when its package moves, and takes the new one when
        /// it next starts, as in `oath update`: no restart under the person
        /// at the keyboard.
        #[arg(long)]
        keep_seat: bool,
    },
    /// Keep the apply on probation. Run it from an SSH session opened
    /// after that apply: the new login is the proof.
    Keep,
    /// Hash linked package and cell trees again against the hash they are
    /// linked as (all of them, or the ids given). Exit 1 if any is damaged.
    Verify {
        ids: Vec<String>,
        /// Print only damaged packages and the count (PID 1's boot check).
        #[arg(short, long)]
        quiet: bool,
    },
    Undo {
        /// PID 1's rollback: undo only the apply on probation, and only once
        /// its time has run out.
        #[arg(long, hide = true)]
        expired: bool,
    },
    /// Move this machine to the newest output published for its profile
    /// (`host:local.profile`) when a key in `host:local.publishers` signed
    /// it (R38). Without --confirm it says what would change.
    Update {
        /// A named output instead of the newest. An older one takes back
        /// only what has no head (linux, oath, firmware, recipes): each
        /// package with a head stays at its head (R72, R81).
        #[arg(long)]
        output: Option<String>,
        /// Reboot through PID 1 when it is done.
        #[arg(long)]
        reboot: bool,
        /// The ESP's node (default: partition 1 of the root disk).
        #[arg(long)]
        esp: Option<String>,
        /// Hold a package at this machine's pin (`pkg:<name>` or `<name>`),
        /// whatever the update would move it to, or keep a service the
        /// release dropped (`svc:<name>`); repeat for more.
        #[arg(long, value_name = "ID")]
        keep: Vec<String>,
        /// Move nothing: list the head of each package here, and record each
        /// one this machine runs as the head it took (/oath/heads.json), as
        /// after `oath publish`. Without --confirm it says what it would
        /// record.
        #[arg(long, conflicts_with_all = ["output", "reboot", "esp", "keep"])]
        record: bool,
    },
    /// Put this machine's pin of a package on an origin as its head (R72),
    /// signed with ~/.config/oath/sign_ed25519, so machines that trust this
    /// key for the name take it at their next `oath update`. Without
    /// --confirm it says what it would publish.
    Publish {
        id: String,
        /// Write the origin's files into this directory instead of uploading.
        #[arg(long)]
        dest: Option<PathBuf>,
        /// When the commit it was built from was made (RFC 3339): orders
        /// it against the head it replaces.
        #[arg(long)]
        committed: Option<String>,
    },
    /// What this machine holds beyond what its packages prescribe (R62):
    /// old versions in the store, filed plans, generations past the newest
    /// five and those since boot. Without --confirm it says what holds each
    /// version and what would go; PID 1 runs it once a boot.
    Reap {
        /// Seconds a tree no pin names yet may wait for one.
        #[arg(long, default_value_t = oath_core::reap::ARRIVING_SECS)]
        arriving: i64,
        /// PID 1's run: a build it finds under `run/` was cut off by the
        /// boot, so its leavings go too.
        #[arg(long, hide = true)]
        boot: bool,
    },
    /// The daily security audit's report, when a key in
    /// `host:local.publishers` signed it (R48, R61): what it found for the
    /// packages here (an id: one package). `--json` is the report itself.
    Audit {
        id: Option<String>,
    },
    /// Put the system this machine runs on a disk (R43). Without a disk it
    /// lists the disks; with one it says what it would erase and write;
    /// with --confirm it wipes that disk, writes an ESP and a btrfs root
    /// (`@`, `@home`), copies this system as a fresh install and makes the
    /// firmware boot it first.
    Install {
        /// The disk: its node (/dev/sdb) or its serial.
        disk: Option<String>,
    },
    Log {
        /// Last N entries (default all).
        #[arg(short = 'n', long)]
        lines: Option<usize>,
    },
    /// Output of a service (`svc:<name>`), the DHCP client (`net:net0`), the
    /// last build of a plan (`plan:<name>`), or PID 1 itself (no id).
    Logs {
        id: Option<String>,
        /// Last N lines.
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
    /// Set, change or remove the password of the desk's user, `home`
    /// (`svc:login`, R42): asked for at the console and by the desk's lock
    /// screen. An empty new password removes it. Off a terminal it reads
    /// the current password (while one is set) and the new one, a line each.
    Password {
        /// Only say whether one is set, with auto-login and the seat.
        #[arg(long)]
        status: bool,
    },
    /// Give the seat (tty1, its screen and keyboard) back to the desk;
    /// `oath desk stop` ends the desk on purpose and gives it to the
    /// console, a shell as home (R42).
    Desk {
        #[arg(value_parser = ["stop"])]
        action: Option<String>,
    },
    /// Wake another machine on this network whose `net:net0.wake` is on
    /// (R84): a Wake-on-LAN magic packet for its MAC, broadcast. Its MAC is
    /// `mac` in its `dev:net0`.
    Wake {
        mac: String,
    },
    /// The verbs as an MCP server on stdin/stdout, one `oath --json`
    /// command per tool call: `ssh -T home@<host> oath mcp`.
    Mcp {
        /// List only the tools that look, and never run sudo.
        #[arg(long)]
        read_only: bool,
    },
    /// Make PID 1 panic, to show that it resumes with every service still
    /// running. Root only. For the harness.
    #[command(hide = true)]
    PanicInit,
    /// Make PID 1 re-execute into another binary as `oath update` does
    /// (R73), keeping every service. Root only. For the harness.
    #[command(hide = true)]
    ReexecInit {
        /// The new PID 1.
        bin: PathBuf,
        /// The hash to ask for; by default the file's own.
        #[arg(long)]
        hash: Option<String>,
    },
    /// Write a fresh catalog from a seed profile, which is data
    /// (`/lib/oath/seed`, or `$OATH_SEED_DIR`), or merge overlays into an
    /// existing one. Build and install time only.
    #[command(hide = true)]
    Seed {
        /// Seed profile for a fresh catalog (`/lib/oath/seed/<name>.json`:
        /// `base`, `desk`).
        #[arg(long)]
        profile: Option<String>,
        /// Overlay JSON (`{"objects": {"kind:name": {…}}}`), applied in order.
        #[arg(long)]
        overlay: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    // A reader that stops early (`oath get pkg:certs | head`) ends `oath`
    // quietly, as it ends any Unix tool. Rust ignores SIGPIPE, which makes
    // a print to a closed stdout panic.
    sigpipe(libc::SIG_DFL);
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            emit_err(&e);
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

/// What a write to a pipe or socket with no reader does: end `oath`
/// (`SIG_DFL`), or fail with EPIPE (`SIG_IGN`).
fn sigpipe(action: libc::sighandler_t) {
    unsafe { libc::signal(libc::SIGPIPE, action) };
}

fn emit_err(e: &Error) {
    let json_mode = std::env::args().any(|a| a == "--json");
    if json_mode {
        let mut o = json!({ "error": e.to_string() });
        if let Some(h) = e.hint_str() {
            o["hint"] = json!(h);
        }
        println!("{}", serde_json::to_string_pretty(&o).unwrap());
    } else {
        eprintln!("{e}");
        if let Some(h) = e.hint_str() {
            eprintln!("hint: {h}");
        }
        if e.exit_code() == EXIT_CONFIRM {
            eprintln!("hint: pass --confirm only if the owner asked. See /oath/INDEX.md safety.");
        }
    }
}

fn run() -> oath_core::Result<i32> {
    let cli = Cli::parse();
    let verb = match &cli.cmd {
        None => "index",
        Some(Cmd::Ls { .. }) => "ls",
        Some(Cmd::Schema { .. }) => "schema",
        Some(Cmd::Get { .. }) => "get",
        Some(Cmd::Set { .. }) => "set",
        Some(Cmd::New { .. }) => "new",
        Some(Cmd::Add { .. }) => "add",
        Some(Cmd::Rm { .. }) => "rm",
        Some(Cmd::Fmt { .. }) => "fmt",
        Some(Cmd::Store { .. }) => "store",
        Some(Cmd::Build { .. }) => "build",
        Some(Cmd::Diff { .. }) => "diff",
        Some(Cmd::Apply { .. }) => "apply",
        Some(Cmd::Keep) => "keep",
        Some(Cmd::Verify { .. }) => "verify",
        Some(Cmd::Undo { .. }) => "undo",
        Some(Cmd::Update { .. }) => "update",
        Some(Cmd::Publish { .. }) => "publish",
        Some(Cmd::Audit { .. }) => "audit",
        Some(Cmd::Reap { .. }) => "reap",
        Some(Cmd::Install { .. }) => "install",
        Some(Cmd::Log { .. }) => "log",
        Some(Cmd::Logs { .. }) => "logs",
        Some(Cmd::Mcp { .. }) => "mcp",
        Some(Cmd::Password { .. }) => "password",
        Some(Cmd::Desk { .. }) => "desk",
        Some(Cmd::Wake { .. }) => "wake",
        Some(Cmd::Seed { .. }) => "seed",
        Some(Cmd::PanicInit) => "panic-init",
        Some(Cmd::ReexecInit { .. }) => "reexec-init",
    };
    tel("oath", "cmd", json!({ "verb": verb, "root": cli.root.display().to_string() }));
    match &cli.cmd {
        Some(Cmd::Mcp { read_only }) => {
            // A server: a client or a child gone is an error it answers,
            // and it writes a plan's text to a child that may exit unread.
            sigpipe(libc::SIG_IGN);
            return mcp::serve(&cli.root, *read_only);
        }
        Some(Cmd::Password { status }) => return login::password(*status, cli.json),
        Some(Cmd::Desk { action }) => return login::desk(action.is_some(), cli.json),
        Some(Cmd::Wake { mac }) => {
            wake::wake(mac)?;
            if cli.json {
                println!("{}", json!({ "mac": mac, "sent": wake::SENDS }));
            } else {
                println!(
                    "sent {mac}'s magic packet {} times to 255.255.255.255:9; a machine that \
                     can wake on it does within seconds",
                    wake::SENDS
                );
            }
            return Ok(0);
        }
        _ => {}
    }
    let actor = Actor { via: cli.via.clone(), ..Actor::current() };
    let cat = Catalog::open(&cli.root)?;
    let out: oath_core::Result<i32> = match cli.cmd {
        None => {
            // The INDEX is the teaching surface; print all of it.
            let text = cat.index_text()?;
            if cli.json {
                println!(
                    "{}",
                    json!({ "index": cat.root().join("INDEX.md").display().to_string(), "text": text })
                );
            } else {
                print!("{text}");
                if !text.ends_with('\n') {
                    println!();
                }
            }
            Ok(0)
        }
        Some(Cmd::Mcp { .. } | Cmd::Password { .. } | Cmd::Desk { .. } | Cmd::Wake { .. }) => {
            unreachable!("served above")
        }
        Some(Cmd::Seed { profile, overlay }) => {
            let fresh = !cli.root.join("INDEX.md").exists();
            if fresh {
                seed_profile(&cli.root, profile.as_deref().unwrap_or(DEFAULT_PROFILE))?;
            } else if profile.is_some() {
                return Err(Error::hint(
                    format!("a catalog already exists at {}", cli.root.display()),
                    "reseeding discards it; pass only --overlay to merge",
                ));
            } else if overlay.is_empty() {
                return Err(Error::hint("nothing to do", "oath seed --overlay <file.json>"));
            }
            for path in &overlay {
                let text = std::fs::read_to_string(path)?;
                let v: Value = serde_json::from_str(&text)?;
                for id in apply_overlay(&cli.root, &v)? {
                    println!("{id}");
                }
            }
            Ok(0)
        }
        Some(Cmd::PanicInit) => {
            use std::io::Read;
            let mut s = std::os::unix::net::UnixStream::connect("/oath/run/init.sock")
                .map_err(|e| Error::hint(format!("init socket: {e}"), "is this Oath?"))?;
            live::send(&s, b"panic\n")?;
            let mut reply = String::new();
            let _ = s.read_to_string(&mut reply);
            if reply.starts_with("ok") {
                return Err(Error::hint("PID 1 did not panic (not root?)", "sudo oath panic-init"));
            }
            println!("PID 1 panicked; `oath logs` shows it resume");
            Ok(0)
        }
        Some(Cmd::ReexecInit { bin, hash }) => {
            let hash = match hash {
                Some(h) => h,
                None => oath_core::file_hash(&std::fs::read(&bin)?),
            };
            match live::reexec_init(&bin, &hash)? {
                live::Reexec::Done => println!("PID 1 runs {} ({hash})", bin.display()),
                live::Reexec::Unsupported => println!(
                    "this PID 1 predates re-executing on request (R73): a reboot runs another"
                ),
            }
            Ok(0)
        }
        Some(Cmd::Ls { origin: true, word, .. }) => {
            add::list(&cli.root, &cat, word.as_deref(), cli.json)
        }
        Some(Cmd::Ls { kind, word, .. }) => {
            let ids = ls(&cat, kind.or(word))?;
            if cli.json {
                let v: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for i in ids {
                    println!("{i}");
                }
            }
            Ok(0)
        }
        Some(Cmd::Schema { kind }) => match kind {
            None => {
                let ks = cat.kinds()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&ks)?);
                } else {
                    for k in ks {
                        println!("{k}");
                    }
                    println!("hint: oath schema <kind>");
                }
                Ok(0)
            }
            Some(k) => {
                let schema = cat.schema_json(&k)?;
                let prose = cat.schema_md(&k).unwrap_or_default();
                if cli.json {
                    let sch: Value = serde_json::from_str(&schema)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "kind": k, "schema": sch, "prose": prose
                        }))?
                    );
                } else {
                    print!("{prose}");
                    println!("---");
                    print!("{schema}");
                }
                Ok(0)
            }
        },
        Some(Cmd::Get { id, actual, desired }) => {
            let id: ObjectId = id.parse()?;
            let obj = cat.get(&id)?;
            if cli.json {
                let mut o = json!({
                    "id": id.to_string(),
                    "meta": {
                        "safety": obj.meta.safety,
                        "status": obj.meta.status,
                    },
                    "desired": obj.desired,
                    "actual": obj.actual,
                });
                if let Some(ov) = overlay::of(&cli.root, &obj) {
                    o["overlay"] = ov;
                }
                if actual {
                    o = obj.actual;
                } else if desired {
                    o = obj.desired;
                }
                println!("{}", serde_json::to_string_pretty(&o)?);
            } else if actual {
                print_val(&obj.actual);
            } else if desired {
                print_val(&obj.desired);
            } else {
                println!("{}  status={}  safety={}", obj.id, obj.meta.status, obj.meta.safety);
                println!("desired:");
                print_val_indent(&obj.desired);
                println!("actual:");
                print_val_indent(&obj.actual);
                if let Some(ov) = overlay::of(&cli.root, &obj) {
                    println!("{}", overlay::line(&obj.id, &ov));
                }
            }
            Ok(0)
        }
        Some(Cmd::Fmt { path, write }) => {
            let raw = std::fs::read_to_string(&path)?;
            let parsed = parse_plan(&raw)?;
            let out = fmt_plan(&parsed);
            if write {
                std::fs::write(&path, &out)?;
            } else {
                print!("{out}");
            }
            Ok(0)
        }
        Some(Cmd::Store { id, src }) => {
            let id: ObjectId = id.parse()?;
            let hash = match id.kind.as_str() {
                KIND_PKG => oath_core::store_pkg(&cli.root, &id.name, &src)?,
                KIND_CELL => oath_core::cell::store(&cli.root, &id.name, &src)?,
                _ => {
                    return Err(Error::hint(
                        "oath store takes pkg:<name> or cell:<name>, and a directory or tar",
                        "oath store pkg:<name> <dir|file.tar>",
                    ));
                }
            };
            tel("oath", "store", json!({ "id": id.to_string(), "hash": hash }));
            if cli.json {
                println!("{}", json!({ "id": id.to_string(), "hash": hash }));
            } else {
                println!("{id} {hash}");
            }
            Ok(0)
        }
        Some(Cmd::Build { id, file, no_pkg }) => match (id, file) {
            (Some(id), None) => {
                let id: ObjectId = id.parse()?;
                if id.kind != KIND_PLAN {
                    return Err(Error::hint(
                        "oath build takes plan:<name> or --file",
                        "oath schema plan",
                    ));
                }
                let r = build_pinned(&cli.root, &id.name, !no_pkg)?;
                print_build(&r, cli.json);
                Ok(0)
            }
            (None, Some(path)) => {
                let r = build_file(&cli.root, &path)?;
                print_build(&r, cli.json);
                Ok(0)
            }
            _ => Err(Error::hint(
                "oath build needs plan:<name> or --file <path>",
                "oath schema plan",
            )),
        },
        Some(Cmd::Set { id, from_json: js, from_file, fields }) => {
            let id: ObjectId = id.parse()?;
            if let Some(path) = from_file {
                if id.kind != KIND_PLAN {
                    return Err(Error::hint("--from-file is for plan:<name>", "oath schema plan"));
                }
                // `-` is a plan's text on stdin (`oath mcp`'s `file_plan`):
                // its canonical form is filed. A file must already be
                // canonical, so its hash is the hash of what is on disk.
                let (plan, bytes) = if path.as_os_str() == "-" {
                    let mut text = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
                    let canon = fmt_plan(&parse_plan(&text)?).into_bytes();
                    (lint_bytes(&canon)?, canon)
                } else {
                    lint_path(&path)?
                };
                if id.name != plan.name {
                    return Err(Error::hint(
                        format!("file name `{}` != {id}", plan.name),
                        "oath schema plan",
                    ));
                }
                let hash = file_plan(&cli.root, &bytes)?;
                if cli.json {
                    println!("{}", json!({ "ok": true, "id": id.to_string(), "hash": hash }));
                } else {
                    println!("{id} {hash}");
                }
                return Ok(0);
            }
            cat.set_fields(&id, field_map(&id, js, fields)?)?;
            if cli.json {
                println!("{}", json!({"ok": true, "id": id.to_string()}));
            }
            Ok(0)
        }
        Some(Cmd::New { id, from_json, fields }) => {
            let id: ObjectId = id.parse()?;
            let map = field_map(&id, from_json, fields)?;
            let named = if id.kind == KIND_PKG && map.is_empty() {
                add::new(&cli.root, &cat, &id, cli.json)?
            } else {
                None
            };
            match named {
                Some(code) => Ok(code),
                None => {
                    cat.new_object(&id, map)?;
                    if cli.json {
                        println!("{}", json!({"ok": true, "id": id.to_string()}));
                    } else {
                        println!("{id} created; {}", on_apply(&id, &cat.get(&id)?.desired));
                    }
                    Ok(0)
                }
            }
        }
        Some(Cmd::Add { names }) => {
            let hooks = live::Live::new(&cli.root);
            add::add(&cli.root, &cat, &actor, &hooks, names, cli.confirm, cli.json)
        }
        Some(Cmd::Rm { id }) => {
            let id: ObjectId = id.parse()?;
            let gone = remove(&cat, &id)?;
            if cli.json {
                println!("{}", json!({"ok": true, "id": id.to_string(), "removed": gone}));
            } else if gone {
                println!("{id} removed: no apply had taken it");
            } else {
                println!("{id} marked for removal; `oath apply {id}` removes it");
            }
            Ok(0)
        }
        Some(Cmd::Diff { id }) => {
            let id = id.map(|s| s.parse()).transpose()?;
            let drift = cat.diff(id.as_ref())?;
            if cli.json {
                let v: Vec<Value> = drift
                    .iter()
                    .map(|d| {
                        json!({
                            "id": d.id.to_string(),
                            "fields": d.fields.iter().map(|(k, n, o)| json!({
                                "field": k, "desired": n, "actual": o
                            })).collect::<Vec<_>>()
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                // Overlays first (R47): `in-sync` stays the last line.
                for (oid, ov) in overlay::all(&cli.root, &cat, id.as_ref())? {
                    println!("{}", overlay::line(&oid, &ov));
                }
                if drift.is_empty() {
                    println!("in-sync");
                }
                for d in &drift {
                    println!("{}", d.id);
                    for (k, n, o) in &d.fields {
                        println!("  {k}: {o} -> {n}");
                    }
                }
            }
            Ok(0)
        }
        Some(Cmd::Apply { ids, probation, keep_seat }) => {
            let ids = if ids.is_empty() {
                None
            } else {
                Some(ids.into_iter().map(|s| s.parse()).collect::<oath_core::Result<Vec<_>>>()?)
            };
            let mut hooks = live::Live::new(&cli.root);
            if keep_seat {
                hooks.follow = live::Follow::KeepSeat;
            }
            let r = cat.apply_with(ids, cli.confirm, probation, &actor, &hooks)?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "generation": r.generation,
                        "ids": r.ids,
                        "probation": r.probation,
                        "slept": r.slept,
                    }))?
                );
            } else if r.ids.is_empty() {
                println!("in-sync");
            } else if r.power_only {
                println!("power change applied without a generation (power is not configuration)");
            } else {
                println!("applied generation {} ({})", r.generation, r.ids.join(", "));
                if let Some(secs) = r.probation {
                    println!(
                        "on probation for {secs} s: open a new SSH session and run \
                         `sudo oath keep` there, or PID 1 undoes generation {}",
                        r.generation
                    );
                }
            }
            if let (Some(s), false) = (&r.slept, cli.json) {
                println!("slept at {} ({}), woke at {}", s.at, s.state, s.woke);
                let mut by = s.wake.clone();
                by.push("the power button".into());
                println!("armed to wake it: {}", by.join(", "));
                if !s.woken_by.is_empty() {
                    println!("woken by: {}", s.woken_by.join(", "));
                }
            }
            Ok(0)
        }
        Some(Cmd::Keep) => {
            let p = oath_core::probation::read(&cli.root)
                .ok_or_else(|| Error::hint("no apply is on probation", "oath log"))?;
            oath_core::probation::check_new_session(&p)?;
            let g = cat.keep(&actor)?;
            if cli.json {
                println!("{}", json!({"kept": g}));
            } else {
                println!("kept generation {g}");
            }
            Ok(0)
        }
        Some(Cmd::Verify { ids, quiet }) => {
            let ids = if ids.is_empty() {
                None
            } else {
                Some(ids.into_iter().map(|s| s.parse()).collect::<oath_core::Result<Vec<_>>>()?)
            };
            let results = cat.verify(ids)?;
            let damaged = results.iter().filter(|r| !r.verified.ok).count();
            if cli.json {
                let v: Vec<Value> = results
                    .iter()
                    .map(|r| json!({ "id": r.id.to_string(), "hash": r.hash, "verified": r.verified }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for r in &results {
                    if r.verified.ok {
                        if !quiet {
                            println!("{} ok {}", r.id, r.hash);
                        }
                    } else {
                        println!("{} DAMAGED: {} hashes to {}", r.id, r.hash, r.verified.got);
                    }
                }
                println!("{} verified, {damaged} damaged", results.len());
            }
            Ok(if damaged > 0 { 1 } else { 0 })
        }
        Some(Cmd::Undo { expired }) => {
            let hooks = live::Live::new(&cli.root);
            let r = if expired {
                let actor = Actor::probation();
                match cat.undo_expired(&actor, &hooks)? {
                    Some(r) => r,
                    None => {
                        println!("nothing on probation has run out");
                        return Ok(0);
                    }
                }
            } else {
                cat.undo(&actor, &hooks)?
            };
            if cli.json {
                println!("{}", json!({"generation": r.generation}));
            } else {
                println!("undid to generation {}", r.generation);
            }
            Ok(0)
        }
        Some(Cmd::Publish { id, dest, committed }) => publish::run(
            &cli.root,
            &cat,
            publish::Opts { id, dest, committed, confirm: cli.confirm, json: cli.json },
        ),
        Some(Cmd::Update { output, reboot, esp, keep, record }) => update::run(
            &cli.root,
            &cat,
            &actor,
            update::Opts {
                confirm: cli.confirm,
                output,
                keep,
                reboot,
                esp,
                record,
                json: cli.json,
            },
        ),
        Some(Cmd::Reap { arriving, boot }) => reap::run(
            &cli.root,
            &cat,
            reap::Opts { confirm: cli.confirm, arriving, boot, json: cli.json },
        ),
        Some(Cmd::Audit { id }) => {
            let id = id.map(|i| i.parse()).transpose()?;
            audit::run(&cat, id.as_ref(), cli.json)
        }
        Some(Cmd::Install { disk }) => install::run(
            &cli.root,
            &cat,
            install::Opts { disk, confirm: cli.confirm, json: cli.json },
        ),
        Some(Cmd::Logs { id, lines }) => {
            let path = match &id {
                Some(id) => {
                    let id: ObjectId = id.parse()?;
                    cli.root.join("log").join(&id.kind).join(format!("{}.log", id.name))
                }
                None => cli.root.join("log/init.log"),
            };
            let mut text = String::new();
            let mut old = path.clone().into_os_string();
            old.push(".1");
            for p in [std::path::PathBuf::from(old), path.clone()] {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    text.push_str(&t);
                }
            }
            if text.is_empty() && !path.is_file() {
                return Err(Error::hint(
                    format!("no log at {}", path.display()),
                    "services log to /oath/log/svc/<name>.log once they have run, \
                     builds to /oath/log/plan/<name>.log",
                ));
            }
            let all: Vec<&str> = text.lines().collect();
            let last = &all[all.len().saturating_sub(lines)..];
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": id,
                        "path": path.display().to_string(),
                        "lines": last,
                    }))?
                );
            } else {
                for l in last {
                    println!("{l}");
                }
            }
            Ok(0)
        }
        Some(Cmd::Log { lines: n }) => {
            let mut lines = cat.log_lines()?;
            if let Some(n) = n {
                lines.drain(..lines.len().saturating_sub(n));
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&lines)?);
            } else {
                for v in lines {
                    println!("{}", serde_json::to_string(&v)?);
                }
            }
            Ok(0)
        }
    };
    match &out {
        Ok(code) => tel("oath", "cmd_done", json!({ "verb": verb, "code": code })),
        Err(e) => tel(
            "oath",
            "cmd_err",
            json!({ "verb": verb, "err": e.to_string(), "code": e.exit_code() }),
        ),
    }
    out
}

fn print_build(r: &BuildReport, json_mode: bool) {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": r.name,
                "produces": r.produces,
                "plan": r.plan_hash,
                "product": r.product,
                "inputs": r.inputs,
                "skipped": r.skipped,
                "log": r.log,
            }))
            .unwrap()
        );
    } else if r.skipped {
        println!("{} {} (cached)", r.produces, r.product);
    } else {
        println!("{} {}", r.produces, r.product);
        println!("log {}", r.log.display());
    }
}

/// What `oath apply` does with an object `oath new` made, by its kind.
fn on_apply(id: &ObjectId, doc: &Value) -> String {
    let apply = format!("`oath apply {id}`");
    let off = |k: &str| doc[k] == json!(false);
    match id.kind.as_str() {
        KIND_SVC if off("enabled") => {
            format!("{apply} takes it, and it does not run (enabled=false)")
        }
        KIND_SVC => format!("{apply} starts it"),
        KIND_PKG if off("present") => format!("{apply} takes it, linked nowhere (present=false)"),
        KIND_PKG => format!("{apply} links it"),
        KIND_CELL if off("present") => {
            format!("{apply} takes it, and `cell run` refuses it (present=false)")
        }
        KIND_CELL => format!("{apply} checks its tree, and `cell run {id}` runs it"),
        _ => format!("{apply} applies it"),
    }
}

/// `oath ls` without --origin: the ids of one kind, or of every kind. A
/// kind the catalog lacks is refused with the kinds it has: the word may
/// be a search for `oath ls --origin` that left out --origin.
fn ls(cat: &Catalog, kind: Option<String>) -> oath_core::Result<Vec<ObjectId>> {
    if let Some(k) = &kind {
        let kinds = cat.kinds()?;
        if !kinds.contains(k) {
            return Err(Error::hint(
                format!("no kind `{k}`: the kinds are {}", kinds.join(", ")),
                format!("oath ls --origin {k}   (what the origins hold)"),
            ));
        }
    }
    cat.ls(kind.as_deref())
}

/// `oath rm`. True when the object is gone already: one no apply has taken
/// is deleted at once, and there is nothing left to apply.
fn remove(cat: &Catalog, id: &ObjectId) -> oath_core::Result<bool> {
    cat.remove_object(id)?;
    Ok(cat.get(id).is_err())
}

/// `--from-json '{…}'` plus `k=v` pairs (v is JSON if it parses, else a string).
fn field_map(
    id: &ObjectId,
    js: Option<String>,
    fields: Vec<String>,
) -> oath_core::Result<Map<String, Value>> {
    let mut map = Map::new();
    if let Some(js) = js {
        let v: Value = serde_json::from_str(&js)?;
        let Some(o) = v.as_object() else {
            return Err(Error::hint(
                "--from-json needs an object",
                format!("oath schema {}", id.kind),
            ));
        };
        map.extend(o.clone());
    }
    for f in fields {
        let Some((k, v)) = f.split_once('=') else {
            return Err(Error::hint(format!("not k=v: {f}"), format!("oath schema {}", id.kind)));
        };
        map.insert(k.to_string(), parse_val(v));
    }
    Ok(map)
}

fn parse_val(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
}

fn print_val(v: &Value) {
    print!("{}", serde_json::to_string_pretty(v).unwrap());
}

fn print_val_indent(v: &Value) {
    if let Some(o) = v.as_object() {
        for (k, val) in o {
            println!("  {k}: {val}");
        }
    } else {
        println!("  {v}");
    }
}

#[allow(dead_code)]
fn _flush() {
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn help(verb: &str) -> String {
        let mut cli = Cli::command();
        let cmd = cli.find_subcommand_mut(verb).unwrap();
        cmd.render_long_help().to_string()
    }

    #[test]
    fn the_help_names_what_each_verb_takes() {
        Cli::command().debug_assert();
        assert!(help("new").contains("svc, pkg or cell"));
        assert!(help("new").contains("takes the package by name"));
        assert!(help("rm").contains("svc, pkg or cell"));
        assert!(help("update").contains("svc:<name>"));
        assert!(!help("update").contains("rollback"));
        assert!(!help("seed").contains("built-in"));
    }

    #[test]
    fn new_says_what_an_apply_does_with_each_kind() {
        let says = |id: &str, doc: Value| on_apply(&id.parse().unwrap(), &doc);
        assert!(says("svc:web", json!({ "enabled": true })).ends_with("starts it"));
        assert!(says("svc:web", json!({ "enabled": false })).contains("does not run"));
        assert!(says("pkg:jq", json!({ "present": true })).ends_with("links it"));
        assert!(says("pkg:jq", json!({ "present": false })).contains("linked nowhere"));
        let cell = says("cell:hamlet", json!({ "present": true }));
        assert!(cell.contains("checks its tree") && cell.contains("`cell run cell:hamlet`"));
        assert!(!cell.contains("starts"), "{cell}");
    }

    #[test]
    fn rm_of_what_no_apply_took_removes_it_at_once() {
        let d = tempfile::tempdir().unwrap();
        oath_core::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        let id: ObjectId = "svc:web".parse().unwrap();
        let doc = json!({ "exec": ["/bin/true"] }).as_object().unwrap().clone();
        cat.new_object(&id, doc).unwrap();
        assert!(remove(&cat, &id).unwrap());
        assert!(cat.get(&id).is_err());
        let sshd: ObjectId = "svc:sshd".parse().unwrap();
        assert!(!remove(&cat, &sshd).unwrap(), "applied: an apply removes it");
        assert_eq!(cat.get(&sshd).unwrap().meta.status, "removing");
    }

    /// `oath ls`'s arguments as clap parses them: --kind, --origin and the
    /// word.
    fn ls_args(args: &[&str]) -> (Option<String>, bool, Option<String>) {
        match Cli::try_parse_from(args).unwrap().cmd {
            Some(Cmd::Ls { kind, origin, word }) => (kind, origin, word),
            _ => panic!("not ls: {args:?}"),
        }
    }

    #[test]
    fn ls_takes_a_kind_as_its_word_or_by_kind() {
        let d = tempfile::tempdir().unwrap();
        oath_core::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        let of = |args: &[&str]| {
            let (kind, origin, word) = ls_args(args);
            assert!(!origin);
            ls(&cat, kind.or(word)).unwrap()
        };
        let by_word = of(&["oath", "ls", "svc"]);
        assert_eq!(by_word, of(&["oath", "ls", "--kind", "svc"]));
        assert!(by_word.iter().any(|i| i.to_string() == "svc:sshd"), "{by_word:?}");
        assert!(by_word.iter().all(|i| i.kind == KIND_SVC), "{by_word:?}");
        assert!(Cli::try_parse_from(["oath", "ls", "--kind", "svc", "pkg"]).is_err());
    }

    #[test]
    fn ls_origin_takes_a_word_or_none() {
        assert_eq!(ls_args(&["oath", "ls", "--origin", "word"]), (None, true, Some("word".into())));
        assert_eq!(ls_args(&["oath", "ls", "--origin"]), (None, true, None));
        assert!(Cli::try_parse_from(["oath", "ls", "--origin", "--kind", "svc"]).is_err());
    }

    #[test]
    fn ls_refuses_a_word_that_is_no_kind_and_names_the_kinds() {
        let d = tempfile::tempdir().unwrap();
        oath_core::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        let (kind, origin, word) = ls_args(&["oath", "ls", "nginx"]);
        assert!(!origin);
        let e = ls(&cat, kind.or(word)).unwrap_err();
        let msg = e.to_string();
        let kinds = cat.kinds().unwrap();
        assert!(kinds.iter().any(|k| k == KIND_SVC) && kinds.iter().any(|k| k == KIND_PKG));
        assert_eq!(msg, format!("no kind `nginx`: the kinds are {}", kinds.join(", ")));
        assert_eq!(e.hint_str(), Some("oath ls --origin nginx   (what the origins hold)"));
    }
}
