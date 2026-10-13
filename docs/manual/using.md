# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 short INDEX
cat /oath/INDEX.md   full INDEX (generated, live)
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
| `oath set <id> k=v` | Write **desired** only. Does not converge. `--from-json '<obj>'` sets several fields |
| `oath diff [id]` | Drift between desired and actual (or last applied, for `svc`) |
| `oath apply [id...]` | Snapshot, then converge the listed ids or all drift |
| `oath undo` | Restore the catalog from the last apply's generation |
| `oath log` | Apply log (JSON lines) |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated, netless build of a pinned plan. Does not link `/bin` |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |

`set` is staging. Nothing is live until `apply`. `set` only changes
objects that exist; see [Catalog](catalog.md) for where objects come
from.

Ids are `kind:name` — `host:local`, `svc:sshd`, `snap:current`,
`pkg:hello`, `net:net0`, `ssh:local`, `dev:vda`, `plan:hello`.

## Safety

`apply` takes a filesystem snapshot first.

- **mutate** — hostname, service enable, package present, the network
  link. Snapshot and go.
- **confirm** — halt, reboot, picking an old boot generation (not
  `undo`). `oath apply` without `--confirm` exits **3** and explains.
  Do not pass `--confirm` unless the owner asked for that class of
  change.

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

Changes that can lock you out (SSH keys, the network link, disabling
`svc:sshd`) are `mutate`. Nothing rolls them back for you. Keep the
serial console or a second session until you have checked.

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is enabled.
SSH is **home** (uid 1, `HOME=/home`, shell `/bin/sh`). `sudo` (no
password) becomes root. The only groups are `root` and `home`. The
apply log records uid and tty. `host:local.env` and `TZ` (from
`host:local.timezone`) are injected by PID 1 into services and written
to root-owned `/etc/profile` for logins. `oath apply` does not write
`$HOME/.profile`.
