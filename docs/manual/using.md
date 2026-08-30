# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 short INDEX
cat /oath/INDEX.md   full INDEX (generated, live)
```

`--json` prints the same facts as JSON. Failures include a **hint**
pointing at the next page (`oath ls`, `oath schema <kind>`, INDEX safety).

Catalog root is `/oath`. `OATH_ROOT` / `--root` is for tests on the
build host, not the appliance.

## Verbs

| Verb | Does |
|------|------|
| `oath ls` | List object ids. `--kind host` (etc.) to filter |
| `oath schema [kind]` | List kinds, or one kind’s prose + JSON Schema |
| `oath get <id>` | Desired + actual + status. `--desired` / `--actual` to clip |
| `oath set <id> k=v` | Write **desired** only. Does not converge |
| `oath diff [id]` | Drift between desired and actual (or last applied, for `svc`) |
| `oath apply [id...]` | Snapshot, then converge listed ids or all drift |
| `oath undo` | Restore the last apply’s generation |
| `oath log` | Apply log (JSON lines) |

`set` is staging. Nothing is live until `apply`.

Ids are `kind:name` — `host:local`, `svc:serial`, `snap:current`,
`pkg:hello`, `net:net0`, `ssh:local`, `dev:card0`.

## Safety

`apply` takes a filesystem snapshot first.

- **mutate** — hostname, service enable, ordinary apply. Snapshot and go.
- **confirm** — halt, reboot, picking an old boot generation (not `undo`).
  `oath apply` without `--confirm` exits **3** and explains. Do not pass
  `--confirm` unless the owner asked for that class of change.

```
oath set host:local hostname=atlas
oath diff
oath apply

oath set host:local power=reboot
oath apply              # refused (exit 3)
oath apply --confirm    # reboots

oath set host:local power=halt
oath apply --confirm    # powers off; QEMU should exit
```

After a reboot apply, desired `power` is set back to `run` so the box
does not loop.

## Who you are

Serial login is **root** (the owner). There is no second Unix user.
The apply log records uid and tty.
