# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 the INDEX (generated, live; also /oath/INDEX.md)
```

`--json` prints the same facts as JSON. Failures include a **hint**
pointing at the next step (`oath ls`, `oath schema <kind>`, INDEX
safety).

The catalog root is `/oath`. `--root` / `OATH_ROOT` is for tests and
for build and install tooling on the build host.

## Verbs

| Verb | Does |
|------|------|
| `oath ls` | List object ids. `--kind host` (etc.) to filter |
| `oath schema [kind]` | List kinds, or one kind's prose + JSON Schema |
| `oath get <id>` | Desired + actual + status. `--desired` / `--actual` to clip |
| `oath set <id> k=v` | Write **desired** only. Does not converge. `--from-json '<obj>'` sets several fields. The result must fit the kind's type |
| `oath new <id> k=v` | Create a `svc` or `pkg` (`--from-json` too). Runs after `apply` |
| `oath rm <id>` | Mark a `svc` or `pkg` for removal. Removed at `apply` |
| `oath diff [id]` | Drift between desired and actual (or last applied, for `svc`) |
| `oath apply [id...]` | Check, snapshot, then converge the listed ids or all drift |
| `oath undo` | Put back what the last apply changed (repeat to go further back) |
| `oath log` | Apply log (JSON lines): `ok`, `error`, `refused`, `undo:N` |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated, netless build of a pinned plan. Does not link `/bin` |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |

`set`, `new` and `rm` are staging. Nothing is live until `apply`, and
PID 1 never runs a spec that was not applied. `set` refuses a key the
kind does not have, or a value of the wrong type
(`oath set svc:hold restart=sometimes` names the field and fails).

Writes to the catalog need root: over SSH, use `sudo oath set`,
`sudo oath apply`, and so on. Reads (`ls`, `get`, `diff`, `log`) do
not. Writers take a lock on `/oath/.lock`, so two `oath` commands never
interleave; the second waits up to 30 s.

Ids are `kind:name` — `host:local`, `svc:sshd`, `snap:current`,
`pkg:hello`, `net:net0`, `ssh:local`, `dev:vda`, `plan:hello`. Names
are letters, digits, `.`, `_` and `-` (at most 64, not starting with
`.`).

## Safety

`apply` checks every object it will touch before it changes anything.
A refused apply (bad value, `wants` cycle, broken package `needs`,
missing `--confirm`) changes nothing and takes no generation. Then it
takes a filesystem snapshot and converges.

- **mutate** — hostname, service enable, package present. Snapshot and
  go.
- **confirm** — halt, reboot, picking an old boot generation (not
  `undo`), and anything that can cut this machine off from SSH:
  disabling, removing or changing `svc:sshd`, emptying `ssh:local`,
  changing `net:net0`. `oath apply` without `--confirm` exits **3** and
  explains. Do not pass `--confirm` unless the owner asked for that
  class of change.

```
oath set host:local hostname=atlas
oath diff
oath apply

oath set host:local power=reboot
oath apply              # refused (exit 3)
oath apply --confirm    # reboots
```

After a reboot apply, desired `power` is set back to `run` so the box
does not loop.

Once a confirm-class change is applied, nothing rolls it back for you.
Keep the serial console or a second session until you have checked.

`oath undo` puts back the spec of the objects the newest apply changed
(an apply that failed counts), converges them, and logs `undo:N`. Run
it again to undo the apply before that. It never rewinds the log, the
package store, or the SSH host keys.

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is enabled.
SSH is **home** (uid 1, `HOME=/home`, shell `/bin/sh`). `sudo` (no
password) becomes root. The only groups are `root` and `home`. The
apply log records uid and tty. `host:local.env` and `TZ` (from
`host:local.timezone`) are injected by PID 1 into services and written
to root-owned `/etc/profile` for logins. `oath apply` does not write
`$HOME/.profile`.
