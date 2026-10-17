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
| `oath keep` | Keep an apply that is on probation. Run it from a new SSH session |
| `oath undo` | Put back what the last apply changed (repeat to go further back) |
| `oath verify [pkg:…]` | Hash linked package trees again against their pins; exit 1 if one changed on disk |
| `oath log` | Apply log (JSON lines): `ok`, `error`, `refused`, `undo:N`, `keep:N` |
| `oath logs [id]` | A service's output (`svc:<name>`), the DHCP client's (`net:net0`), the last build of a plan (`plan:<name>`), or PID 1's (no id). `-n N` lines, default 50 |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated, netless build of a pinned plan. Does not link `/bin` |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |

`set`, `new` and `rm` are staging. Nothing is live until `apply`, and
PID 1 never runs a spec that was not applied. `set` refuses a key the
kind does not have, or a value of the wrong type
(`oath set svc:time restart=sometimes` names the field and fails).

Writes to the catalog need root: over SSH, use `sudo oath set`,
`sudo oath apply`, and so on. Reads (`ls`, `get`, `diff`, `log`) do
not. Writers take a lock on `/oath/.lock`, so two `oath` commands never
interleave; the second waits up to 30 s.

Ids are `kind:name` — `host:local`, `svc:sshd`, `snap:current`,
`pkg:curl`, `net:net0`, `ssh:local`, `dev:net0`, `plan:hello`. Names
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
  disabling, removing or changing `svc:sshd`, dropping a key from
  `ssh:local`, changing `net:net0`. `oath apply` without `--confirm` exits **3** and
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
does not loop. The apply asks PID 1 for the reboot; PID 1 stops the
services, syncs and remounts `/` read-only first. A power change on its
own takes no generation (power is not configuration); plain
`sudo reboot` and `sudo poweroff` do the same.

A change that can cut off SSH goes on **probation** once applied:

```
sudo oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
sudo oath apply --confirm              # on probation for 180 s
ssh home@192.168.1.20 sudo oath keep   # a new session: kept
```

If no new session runs `sudo oath keep` in time (`--probation <secs>`,
10–3600, default 180), PID 1 undoes the apply and logs `undo:N` by
`probation`. A reboot before `keep` undoes it too. The session you
applied from cannot keep it: only a login made after the apply proves
the machine is still reachable. While a probation is open nothing else
applies; `sudo oath undo` takes it back at once.

`oath undo` puts back the spec of the objects the newest apply changed
(an apply that failed counts), converges them, and logs `undo:N`. Run
it again to undo the apply before that. It never rewinds the log, the
package store, or the SSH host keys.

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is enabled.
The screen and keyboard are **home** when `svc:screen` is enabled. SSH is **home** (uid 1, `HOME=/home`, shell `/bin/sh`). `sudo` (no
password) becomes root. The only groups are `root` and `home`. The
apply log records uid and tty. `host:local.env` and `TZ` (from
`host:local.timezone`) are injected by PID 1 into services and written
to root-owned `/etc/profile` for logins. `oath apply` does not write
`$HOME/.profile`.
