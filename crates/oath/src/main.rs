use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use oath_core::{
    build_file, build_pinned, file_plan, fmt_plan, lint_path, parse_plan, seed, tel, Actor,
    BuildReport, Catalog, Error, ObjectId, DEFAULT_ROOT, EXIT_CONFIRM, KIND_PLAN,
};
use serde_json::{json, Map, Value};

mod live;

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
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Ls {
        #[arg(long)]
        kind: Option<String>,
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
    /// Write a canonical plan.plan to stdout (or -w in place).
    Fmt {
        path: PathBuf,
        #[arg(short = 'w')]
        write: bool,
    },
    /// Isolated plan build. Does not link /bin.
    Build {
        id: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    Diff {
        id: Option<String>,
    },
    Apply {
        ids: Vec<String>,
    },
    Undo,
    Log,
    #[command(hide = true)]
    Seed,
    /// Hash old-layout store slots into actual.hash without moving trees.
    #[command(hide = true)]
    StampHash,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            emit_err(&e);
            ExitCode::from(e.exit_code() as u8)
        }
    }
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
        Some(Cmd::Fmt { .. }) => "fmt",
        Some(Cmd::Build { .. }) => "build",
        Some(Cmd::Diff { .. }) => "diff",
        Some(Cmd::Apply { .. }) => "apply",
        Some(Cmd::Undo) => "undo",
        Some(Cmd::Log) => "log",
        Some(Cmd::Seed) => "seed",
        Some(Cmd::StampHash) => "stamp-hash",
    };
    tel("oath", "cmd", json!({ "verb": verb, "root": cli.root.display().to_string() }));
    let cat = Catalog::open(&cli.root)?;
    let out: oath_core::Result<i32> = match cli.cmd {
        None => {
            let text = cat.index_text()?;
            let short: String = text.lines().take(24).collect::<Vec<_>>().join("\n");
            if cli.json {
                println!(
                    "{}",
                    json!({ "index": cat.root().join("INDEX.md").display().to_string(), "text": text })
                );
            } else {
                println!("{short}");
                println!();
                println!("full INDEX: {}", cat.root().join("INDEX.md").display());
            }
            Ok(0)
        }
        Some(Cmd::Seed) => {
            seed(&cli.root)?;
            Ok(0)
        }
        Some(Cmd::StampHash) => {
            let ids = cat.ls(Some("pkg"))?;
            let mut n = 0u32;
            for id in ids {
                if let Some(h) = oath_core::stamp_unhashed(&cli.root, &id.name)? {
                    println!("{} {h}", id);
                    n += 1;
                }
            }
            if cli.json {
                println!("{}", json!({ "stamped": n }));
            } else if n == 0 {
                println!("in-sync");
            }
            Ok(0)
        }
        Some(Cmd::Ls { kind }) => {
            let ids = cat.ls(kind.as_deref())?;
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
        Some(Cmd::Build { id, file }) => match (id, file) {
            (Some(id), None) => {
                let id: ObjectId = id.parse()?;
                if id.kind != KIND_PLAN {
                    return Err(Error::hint(
                        "oath build takes plan:<name> or --file",
                        "oath schema plan",
                    ));
                }
                let r = build_pinned(&cli.root, &id.name)?;
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
                    return Err(Error::hint(
                        "--from-file is for plan:<name>",
                        "oath schema plan",
                    ));
                }
                let (plan, bytes) = lint_path(&path)?;
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
            let mut map = Map::new();
            if let Some(js) = js {
                let v: Value = serde_json::from_str(&js)?;
                if let Some(o) = v.as_object() {
                    map.extend(o.clone());
                } else {
                    return Err(Error::hint(
                        "set --json needs an object",
                        format!("oath schema {}", id.kind),
                    ));
                }
            }
            for f in fields {
                let Some((k, v)) = f.split_once('=') else {
                    return Err(Error::hint(
                        format!("not k=v: {f}"),
                        format!("oath schema {}", id.kind),
                    ));
                };
                map.insert(k.to_string(), parse_val(v));
            }
            cat.set_fields(&id, map)?;
            if cli.json {
                println!("{}", json!({"ok": true, "id": id.to_string()}));
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
            } else if drift.is_empty() {
                println!("in-sync");
            } else {
                for d in &drift {
                    println!("{}", d.id);
                    for (k, n, o) in &d.fields {
                        println!("  {k}: {o} -> {n}");
                    }
                }
            }
            Ok(0)
        }
        Some(Cmd::Apply { ids }) => {
            let ids = if ids.is_empty() {
                None
            } else {
                Some(ids.into_iter().map(|s| s.parse()).collect::<oath_core::Result<Vec<_>>>()?)
            };
            let hooks = live::Live { catalog_root: cli.root.clone() };
            let r = cat.apply(ids, cli.confirm, &Actor::current(), &hooks)?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "generation": r.generation,
                        "ids": r.ids,
                    }))?
                );
            } else if r.ids.is_empty() {
                println!("in-sync");
            } else {
                println!("applied generation {} ({})", r.generation, r.ids.join(", "));
            }
            Ok(0)
        }
        Some(Cmd::Undo) => {
            let hooks = live::Live { catalog_root: cli.root.clone() };
            let r = cat.undo(&Actor::current(), &hooks)?;
            if cli.json {
                println!("{}", json!({"generation": r.generation}));
            } else {
                println!("undid to generation {}", r.generation);
            }
            Ok(0)
        }
        Some(Cmd::Log) => {
            let lines = cat.log_lines()?;
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
                "plan": r.plan_hash,
                "product": r.product,
                "inputs": r.inputs,
                "skipped": r.skipped,
            }))
            .unwrap()
        );
    } else if r.skipped {
        println!("pkg:{} {} (cached)", r.name, r.product);
    } else {
        println!("pkg:{} {}", r.name, r.product);
    }
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
