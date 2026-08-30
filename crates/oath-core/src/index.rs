use crate::catalog::Catalog;
use crate::Result;

pub fn generate(cat: &Catalog) -> Result<String> {
    let kinds = cat.kinds()?;
    let mut kinds_block = String::new();
    for k in &kinds {
        let line = kind_line(k);
        kinds_block.push_str(&format!("- `{k}` — {line}  (`oath schema {k}`)\n"));
    }
    if kinds_block.is_empty() {
        kinds_block.push_str("- (no kinds present)\n");
    }

    Ok(format!(
        r#"# Oath

You are on **Oath**. Do not assume Debian, Arch, NixOS, Alpine, or Ubuntu.

Do **not** edit `/etc` or random files. Use `oath`.

## Verbs

    oath                 short INDEX (this file is {index})
    oath ls              list objects  (optional --kind)
    oath schema [kind]   kinds, or one kind's schema + prose
    oath get <id>        desired + actual
    oath set <id> k=v    write desired only
    oath diff [id]       drift
    oath apply [id...]   snapshot, then converge
    oath undo            restore the last apply's snapshot
    oath log             apply log

`--json` prints the same facts as JSON. `oath --help` for flags.

## Kinds here

{kinds}

## Safety

`oath apply` takes a filesystem snapshot first. Halt, wipe, and boot
generation changes (other than `oath undo`) need `--confirm`. Do **not**
pass `--confirm` unless the owner asked for that class of change.

## Where you are

Serial console on the appliance. Root is the owner. The catalog lives
at `/oath`.
"#,
        index = cat.root().join("INDEX.md").display(),
        kinds = kinds_block.trim_end(),
    ))
}

fn kind_line(kind: &str) -> &'static str {
    match kind {
        "host" => "this machine (hostname, power)",
        "svc" => "a process PID 1 supervises",
        "snap" => "btrfs generations (apply / undo)",
        "pkg" => "a package (store + /bin links)",
        "net" => "a network link (net0, static or dhcp)",
        "ssh" => "owner SSH public keys (root / dropbear)",
        "dev" => "a hardware device (inventory)",
        _ => "see schema",
    }
}
