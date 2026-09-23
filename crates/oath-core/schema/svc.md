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
- `user` — `root` or `home`. Oath has those two Unix accounts and no
  others (`home` is uid 1, gid 1). Absent means `root`, so `svc:sshd`,
  `svc:seatd`, and the rest of today's non-seat services stay root.
  River, Hyprland, the Omarchy shell, the Sola stack, and PipeWire
  still run as `home` when this field is absent, which keeps an
  existing desk catalog on boot. An explicit value is the account
  that process uses, including `user=root` on a seat name. Wicket's
  control plane, Caddy, and Postgres use `home` on a Wicket image.
  This field does not add a `wicket` user or a `postgres` user.
- `env` — string map, empty when absent. PID 1 puts these in the
  process environment after `host:local.env`. The same name on this
  object wins. Example: `WICKET_STATE_DIR=/var/lib/wicket`.
- `bind_low_ports` — boolean, false when absent. When true, a
  service that runs as `home` keeps `CAP_NET_BIND_SERVICE` and may
  bind ports below 1024 (Caddy on 80 and 443). A root service can
  already bind those ports. A `home` service with this field false
  or absent does not receive that capability.

Changing `user`, `env`, or `bind_low_ports` restarts the process on
the next converge. Deploy-key root SSH, the Wicket disk layout, and
rootless Podman are later slices and are not fields on this object.

`svc:backup` sleeps until **04:00 US Mountain** then sends
(`restart=always`, default `enabled=false` so QEMU does not hit the
NAS). `exec` is `/lib/oath/backup-daily` plus an NFS spec
(`10.0.0.12:/mnt/alpha/backup/canto`). Enable and `oath apply` starts
the sleeper (no send until 4am). Manual send:
`/lib/oath/backup-send <spec>`. Last send is `actual` plus `last.json`.

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
