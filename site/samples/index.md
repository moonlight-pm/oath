# Oath

You are on **Oath**. Do not assume Debian, Arch, NixOS, Alpine, or Ubuntu.

Do **not** edit `/etc` or random files. Use `oath`.

## Verbs

    oath                 this INDEX (the file is /oath/INDEX.md)
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

## Kinds here

- `dev` — a hardware device (inventory)  (`oath schema dev`)
- `host` — this machine (hostname, power, env, timezone)  (`oath schema host`)
- `net` — a network link (net0, dhcp or static)  (`oath schema net`)
- `pkg` — a package (store + /bin links)  (`oath schema pkg`)
- `plan` — a build plan (hash-pinned file; oath build)  (`oath schema plan`)
- `snap` — btrfs generations (apply / undo)  (`oath schema snap`)
- `ssh` — owner SSH public keys (login as home)  (`oath schema ssh`)
- `svc` — a process PID 1 supervises  (`oath schema svc`)

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
(sshd, `ssh:local` keys, `net:net0`, a new `pkg:dropbear` hash) need
`--confirm`. A service restarts when an apply moves the package its
program comes from.
