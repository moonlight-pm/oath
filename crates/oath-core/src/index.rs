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

    oath                 this INDEX (the file is {index})
    oath ls              list objects  (optional --kind)
    oath schema [kind]   kinds, or one kind's schema + prose
    oath get <id>        desired + actual
    oath set <id> k=v    write desired only (checked against the kind's type)
    oath new <id> k=v    create a svc or pkg (runs after apply)
    oath rm <id>         remove a svc or pkg (after apply)
    oath diff [id]       drift
    oath apply [id...]   check, snapshot, then converge
    oath undo            put back the spec the last apply changed, and converge
    oath log             apply log
    oath fmt <path>      canonical plan.plan
    oath build           isolated plan build (no /bin link)

`--json` prints the same facts as JSON. `oath --help` for flags.

## Kinds here

{kinds}

## Safety

`oath apply` checks everything first (a refusal changes nothing), then
takes a filesystem snapshot. Halt, reboot, boot generation changes
(other than `oath undo`) and changes that can cut off SSH need
`--confirm`. Do **not** pass `--confirm` unless the owner asked for that
class of change.

## Where you are

Serial is **root** (break-glass). SSH is **home** (`HOME=/home`);
`sudo` has no password. The catalog lives at `/oath`. `/home` is the
`home` account's directory. There is no `/usr`; helpers are
`/lib/oath`.

`oath new` makes a `svc` or `pkg`; `oath rm` removes one. Both, like
`set`, only change the spec: nothing happens until `oath apply`, and
`oath undo` takes back the last apply. Changes that can cut off SSH
(sshd, `ssh:local` keys, `net:net0`) need `--confirm`.
"#,
        index = cat.root().join("INDEX.md").display(),
        kinds = kinds_block.trim_end(),
    ))
}

fn kind_line(kind: &str) -> &'static str {
    match kind {
        "host" => "this machine (hostname, power, env, timezone)",
        "svc" => "a process PID 1 supervises",
        "snap" => "btrfs generations (apply / undo)",
        "pkg" => "a package (store + /bin links)",
        "net" => "a network link (net0, dhcp or static)",
        "ssh" => "owner SSH public keys (login as home)",
        "dev" => "a hardware device (inventory)",
        "plan" => "a build plan (hash-pinned file; oath build)",
        _ => "see schema",
    }
}
