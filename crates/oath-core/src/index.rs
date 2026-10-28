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
    oath keep            keep an apply on probation (from a new SSH session)
    oath undo            put back the spec the last apply changed, and converge
    oath verify          hash linked package trees again against their pins
    oath update          what the newest output for this machine would change;
                         `sudo oath update --confirm [--reboot]` moves to it
    oath log             apply log
    oath logs [id]       output of svc:<name>, net:net0, a plan's last build
                         (plan:<name>), or PID 1 (no id)
    oath fmt <path>      canonical plan.plan
    oath build           isolated plan build (no /bin link; log in /oath/log/plan/)
    oath store pkg:<name> <dir|tar>
                         put a tree built elsewhere into the store and print
                         its hash; `oath set pkg:<name> hash=…` pins it
    oath mcp             these verbs as an MCP server on stdin/stdout
                         (ssh -T home@<host> oath mcp)

`--json` prints the same facts as JSON. `oath --help` for flags.
{probation}{damage}{recipes}
## Kinds here

{kinds}

## Safety

`oath apply` checks everything first (a refusal changes nothing), then
takes a filesystem snapshot. Halt, reboot, boot generation changes
(other than `oath undo`) and changes that can cut off SSH need
`--confirm`. Do **not** pass `--confirm` unless the owner asked for that
class of change. A change that can cut off SSH then goes on probation:
PID 1 undoes it after 180 s (`--probation <secs>`), or at the next
boot, unless `sudo oath keep` runs in an SSH session opened after the
apply. Nothing else applies while it is open.

## Where you are

Serial is **root** (break-glass). SSH is **home** (`HOME=/home`);
`sudo` has no password. The catalog lives at `/oath`. `/home` is the
`home` account's directory. There is no `/usr`; helpers are
`/lib/oath`.

`oath new` makes a `svc` or `pkg`; `oath rm` removes one. Both, like
`set`, only change the spec: nothing happens until `oath apply`, and
`oath undo` takes back the last apply. Changes that can cut off SSH
(sshd, `ssh:local` keys, `net:net0`, a new `pkg:openssh` hash) need
`--confirm`. A service restarts when an apply moves the package its
program comes from.

Updates come from the origins (`host:local.origins`): a build host
publishes each output it checked, signed, and `oath update` takes the
newest for `host:local.profile` when a key in `host:local.publishers`
signed it. It moves every package in one generation, then the kernel,
initrd and PID 1 for the next boot; the boot before stays in the menu.
"#,
        index = cat.root().join("INDEX.md").display(),
        kinds = kinds_block.trim_end(),
        probation = probation_block(cat),
        damage = damage_block(cat),
        recipes = recipe_block(cat),
    ))
}

/// Packages that are a plan here and not a shipped pack (R23): the owner
/// builds them on this machine.
fn recipe_block(cat: &Catalog) -> String {
    let mut lines = Vec::new();
    for id in cat.ls(Some("plan")).unwrap_or_default() {
        let pkg = crate::ObjectId::new("pkg", &id.name);
        let Ok(obj) = cat.get(&pkg) else { continue };
        if obj.desired.get("present") != Some(&serde_json::json!(false)) {
            continue;
        }
        let what = obj.desired.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let n = &id.name;
        lines.push(format!(
            "- `{pkg}` {what} `sudo oath build plan:{n}`, then `sudo oath set {pkg} \
             present=true` and `sudo oath apply {pkg}`.\n"
        ));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\n## Recipes\n\n\
         These packages are not in the image: their licences do not allow \
         redistribution, or (coding agents) the machine takes the vendor's own \
         build. Each is a plan that fetches the vendor's file and builds the \
         package on this machine.\n\n{}",
        lines.concat()
    )
}

/// An open probation, said where an agent looks first.
fn probation_block(cat: &Catalog) -> String {
    let Some(p) = crate::probation::read(cat.root()) else { return String::new() };
    let until = chrono::DateTime::parse_from_rfc3339(&p.applied)
        .map(|t| {
            (t + chrono::Duration::seconds(p.seconds as i64))
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_else(|_| format!("{} s after {}", p.seconds, p.applied));
    format!(
        "\n## Probation\n\n\
         Generation {} ({}) can cut off SSH and is on probation until {until}. \
         Open a **new** SSH session and run `sudo oath keep` there to keep it; \
         otherwise PID 1 undoes it. `sudo oath undo` takes it back now.\n",
        p.generation,
        p.ids.join(", "),
    )
}

/// Packages whose store tree no longer hashes to its pin.
fn damage_block(cat: &Catalog) -> String {
    let mut bad = Vec::new();
    for id in cat.ls(Some("pkg")).unwrap_or_default() {
        let Ok(obj) = cat.get(&id) else { continue };
        if obj.actual.pointer("/verified/ok") == Some(&serde_json::json!(false)) {
            let got = obj.actual.pointer("/verified/got").and_then(|g| g.as_str()).unwrap_or("");
            bad.push(format!("- `{id}` hashes to `{got}`, not its pin\n"));
        }
    }
    if bad.is_empty() {
        return String::new();
    }
    format!(
        "\n## Store damage\n\n\
         These package trees changed on disk since they were stored. Do not trust \
         what they run. Fetch them again (`url`) or reinstall; `oath verify` checks \
         again.\n\n{}",
        bad.concat()
    )
}

fn kind_line(kind: &str) -> &'static str {
    match kind {
        "host" => "this machine (hostname, power, env, timezone, mounts)",
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
