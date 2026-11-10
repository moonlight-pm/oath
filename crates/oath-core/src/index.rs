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
    oath ls --origin [word]  what the origins hold; `*` is here
    oath add <name>...   install by name, with what it needs (`oath new pkg:<name>`
                         with no fields makes them without applying)
    oath schema [kind]   kinds, or one kind's schema + prose
    oath get <id>        desired + actual
    oath set <id> k=v    write desired only (checked against the kind's type)
    oath new <id> k=v    create a svc, pkg or cell (runs after apply)
    oath rm <id>         remove a svc, pkg or cell (after apply)
    oath diff [id]       drift
    oath apply [id...]   check, snapshot, then converge
    oath keep            keep an apply on probation (from a new SSH session)
    oath undo            put back the spec the last apply changed, and converge
    oath verify          hash present package and cell trees again against
                         their pins
    oath audit [pkg:x]   the daily security audit's findings for what is here
    oath reap            what is on disk that no package prescribes;
                         `sudo oath reap --confirm` removes it (each boot does)
    oath update          what the newest output for this machine, and each
                         package's newest signed head (R81), would change;
                         `sudo oath update --confirm [--reboot]` moves to them
    oath log             apply log
    oath logs [id]       output of svc:<name>, net:net0, a plan's last build
                         (plan:<name>), or PID 1 (no id)
    oath fmt <path>      canonical plan.plan
    oath build           isolated plan build (no /bin link; log in /oath/log/plan/)
    oath store pkg:<name>|cell:<name> <dir|tar>
                         put a tree built elsewhere into the store and print
                         its hash; `oath set pkg:<name> hash=…` pins it
    oath mcp             these verbs as an MCP server on stdin/stdout
                         (ssh -T home@<host> oath mcp)
    oath password        set, change or remove home's password
                         (`--status`: is one set, auto-login, the seat)
    oath desk [stop]     give the seat (tty1) back to the desk, or end the
                         desk and give it to the console (a shell as home)

`--json` prints the same facts as JSON. `oath --help` for flags.
{probation}{damage}{recipes}
## Kinds here

{kinds}

## Safety

`oath apply` checks everything first (a refusal changes nothing), then
takes a filesystem snapshot. Halt, reboot and changes that can cut off
SSH need `--confirm`. Do **not** pass `--confirm` unless the owner asked for that
class of change. A change that can cut off SSH then goes on probation:
PID 1 undoes it after 180 s (`--probation <secs>`), or at the next
boot, unless `sudo oath keep` runs in an SSH session opened after the
apply. Nothing else applies while it is open.

## Where you are

Serial is **root** (break-glass). SSH is **home** (`HOME=/home`);
`sudo` has no password. `home` has none either until `oath password`
sets one (`svc:login` keeps its hash, root's alone): the console asks
for it when `host:local.autologin` is false, and so does a desk's lock
screen; SSH (keys only) and `sudo` never do. The catalog lives at `/oath`. `/home` is the
`home` account's directory. There is no `/usr`; helpers are
`/lib/oath`.

`oath new` makes a `svc`, `pkg` or `cell`; `oath rm` removes one. Both, like
`set`, only change the spec: nothing happens until `oath apply`, and
`oath undo` takes back the last apply. Changes that can cut off SSH
(sshd, `ssh:local` keys, `net:net0`, a new `pkg:openssh` hash) need
`--confirm`. A service restarts when an apply moves the package its
program comes from.

Updates come from the origins (`host:local.origins`): a build host
publishes each output it checked, signed, and `oath update` takes the
newest for `host:local.profile` when a key in `host:local.publishers`
signed it. It moves every package in one generation and writes the
kernel and initrd for the next boot; the boot before stays in the menu.
PID 1 re-executes into the output's at once, every service running
(R73), unless `--reboot` boots it.
A package published on its own has a head on the origins
(`pkg/<name>.json`, R72): `oath update` moves each package to the newest
head a key may sign for that name (a `names="…"` list before a key scopes
it), re-pointing a need only when the package still finds every library
it loads. `oath publish pkg:<name>` puts this machine's pin up as its head.
"#,
        index = cat.root().join("INDEX.md").display(),
        kinds = kinds_block.trim_end(),
        probation = probation_block(cat),
        damage = damage_block(cat),
        recipes = recipe_block(cat),
    ))
}

/// Packages that are a plan here and not a shipped pack (R23): the owner
/// builds them on this machine. A recipe pins its vendor's file as
/// `pkg:<name>-src`; any other absent product is what `oath build` left of
/// the owner's own plan.
fn recipe_block(cat: &Catalog) -> String {
    let mut lines = Vec::new();
    for id in cat.ls(Some("plan")).unwrap_or_default() {
        let pkg = crate::ObjectId::new("pkg", &id.name);
        let Ok(obj) = cat.get(&pkg) else { continue };
        if obj.desired.get("present") != Some(&serde_json::json!(false)) {
            continue;
        }
        if cat.get(&crate::ObjectId::new("pkg", &format!("{}-src", id.name))).is_err() {
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

/// Packages and cells whose store tree no longer hashes to its pin, or is
/// gone from the store, each with its repair.
fn damage_block(cat: &Catalog) -> String {
    let mut bad = Vec::new();
    let cells = cat.ls(Some(crate::KIND_CELL)).unwrap_or_default();
    for id in cat.ls(Some(crate::KIND_PKG)).unwrap_or_default().into_iter().chain(cells) {
        let Ok(obj) = cat.get(&id) else { continue };
        if obj.actual.pointer("/verified/ok") != Some(&serde_json::json!(false)) {
            continue;
        }
        let got = obj.actual.pointer("/verified/got").and_then(|g| g.as_str()).unwrap_or("");
        // `oath verify` records a tree gone from the store as `missing`.
        let what = if got == "missing" {
            "is gone from the store".to_string()
        } else {
            format!("hashes to `{got}`, not its pin")
        };
        let fix = if id.kind == crate::KIND_CELL {
            format!("sudo oath store {id} <dir|file.tar>")
        } else {
            format!("sudo oath apply {id}")
        };
        bad.push(format!("- `{id}` {what}: `{fix}`\n"));
    }
    if bad.is_empty() {
        return String::new();
    }
    format!(
        "\n## Store damage\n\n\
         These trees changed on disk, or are gone from the store, since they were stored. \
         Do not trust what a changed one runs. Apply fetches a package's tree again from its \
         `url` or `host:local.origins` and puts the new copy in place only once it hashes to \
         its pin; if nothing serves one that does, apply fails and says what each answered. \
         A cell comes from no origin: `oath store` of the project's tree puts it back. \
         `oath verify` checks again.\n\n{}",
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
        "cell" => "one project's software, run only in a cell (never linked or published)",
        _ => "see schema",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{status_path, write_json, ObjectId};
    use serde_json::json;

    #[test]
    fn store_damage_names_each_tree_and_its_repair() {
        let d = tempfile::tempdir().unwrap();
        crate::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        assert!(!generate(&cat).unwrap().contains("## Store damage"));
        let cell: ObjectId = "cell:hamlet".parse().unwrap();
        let spec = json!({ "present": false });
        cat.new_object(&cell, spec.as_object().unwrap().clone()).unwrap();
        let got = format!("sha256-{}", "b".repeat(64));
        let found = [("pkg:less", "missing"), ("pkg:bash", got.as_str()), ("cell:hamlet", &got)];
        for (id, got) in found {
            let id: ObjectId = id.parse().unwrap();
            let p = status_path(d.path(), &id);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let actual = json!({ "verified": { "ok": false, "at": "", "got": got } });
            write_json(&p, &actual).unwrap();
        }
        let idx = generate(&cat).unwrap();
        let block = idx.split("## Store damage").nth(1).unwrap_or("");
        for line in [
            "- `pkg:less` is gone from the store: `sudo oath apply pkg:less`".to_string(),
            format!("- `pkg:bash` hashes to `{got}`, not its pin: `sudo oath apply pkg:bash`"),
            format!(
                "- `cell:hamlet` hashes to `{got}`, not its pin: \
                 `sudo oath store cell:hamlet <dir|file.tar>`"
            ),
        ] {
            assert!(block.contains(&line), "{line}\n{block}");
        }
        assert!(!block.contains("reinstall"), "{block}");
        // check/cases.toml's store-damage-is-caught reads the first with `grep -A4`.
        let after: Vec<&str> = idx.lines().skip_while(|l| *l != "## Store damage").collect();
        assert!(after[4].starts_with("- `pkg:"), "{after:?}");
    }
}
