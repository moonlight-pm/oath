# svc

A process supervised by PID 1. PID 1 is not an object; it is the engine
that converges `svc:*`. There is no unit file.

**When to use:** start, stop, or change a supervised process.

**When not:** do not hand-write init dialects. Do not add a service by
editing `/etc`.

## Fields

- `exec` — argv, `exec[0]` absolute.
- `wants` — other `svc` ids that should be up first. PID 1 starts them
  in that order. Cycles refuse `oath apply`. Disabled/unknown wants
  are skipped (ordering, not Requires). `svc:hold` wants `svc:serial`.
- `restart` — `never` | `always` | `on-failure`. `never` is a oneshot:
  PID 1 starts it when enabled, does not start it again after it
  exits until `enabled` goes false then true (or the next boot if it
  is still enabled).
- `enabled` — if false, the process is not running.

`svc:backup` is a oneshot (`restart=never`, default `enabled=false`).
`exec` is `/lib/oath/backup-send` plus an NFS spec
(`10.0.0.12:/mnt/alpha/backup/canto`). Enable and `oath apply` sends
one full generation to that directory (overwrite). Last send is
`actual` plus `last.json` (generation, checksum).

Safety: `mutate`. Apply writes desired, then notifies PID 1.

## Example

```
oath ls --kind svc
oath get svc:serial
oath set svc:serial enabled=true
oath apply svc:serial
```

The appliance ships `svc:serial` (serial root shell; graphical
stack runs as `home`), `svc:hold`
(`/bin/sleep 86400000`, for start/stop tests), `svc:sshd`,
`svc:seatd`, `svc:river` (patched River on `dev:card0`; wants
seatd; libinput via libudev-zero), and the Sola session stack
(`svc:sola-bus`, `svc:sola-call`, `svc:sola-river` the Wayland
bridge, `svc:sola-shell`, `svc:sola-session`), and `svc:backup`
(off-box NFS send; off until you enable it). Do not run Sola’s
process manager.
Do not disable serial unless you have another console.

```
oath set svc:hold enabled=false
oath apply svc:hold
```
