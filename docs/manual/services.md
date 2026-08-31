# Services

PID 1 is **oath-init** (`/usr/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/usr/lib/oath/serial-login` | enabled, `restart=always` | Root shell on the QEMU serial. Do not disable it unless you have another console. |
| `svc:hold` | `/bin/sleep 86400000` | enabled, `restart=always` | Harmless sleeper for start/stop. |
| `svc:sshd` | dropbear | enabled, `restart=always` | Keys in `ssh:local`. Password off. |
| `svc:seatd` | `/bin/seatd` | enabled, `restart=always` | Seat for DRM. `svc:river` wants this. |
| `svc:river` | `/bin/river` | enabled, `restart=always` | Patched River on `dev:card0` (pixman). libinput via libudev-zero (`dev:kbd0` / `dev:mouse0`). Socket under `/run/user/0`. |
| `svc:sola-bus` | `/bin/sola-bus` | enabled, `restart=always` | Sola IPC bus. Socket `/run/user/0/sola-bus`. |
| `svc:sola-call` | `/bin/sola-call` | enabled, `restart=always` | Sola call host. Socket `/run/user/0/sola-call`. |
| `svc:sola-river` | `/bin/sola-river` | enabled, `restart=always` | Bridge (bus ↔ Wayland). Wants `svc:river` + bus + call. Not the compositor. |
| `svc:sola-shell` | `/bin/sola-shell` | enabled, `restart=always` | Iced menubar / launcher. Wants river + bus + call + the bridge. Software GL. |

PID 1 starts enabled services at boot, **stops disabled ones** on
converge, reaps, and restarts per policy (`never` / `always` /
`on-failure`).

`oath apply` on a `svc` writes desired, notifies PID 1 on
`/oath/run/init.sock`, and waits until actual `state` is `running` or
`stopped`. `oath undo` notifies PID 1 after restoring the catalog.

`wants` is start order: enabled wanted services start first. Cycles
refuse apply. `svc:hold` wants `svc:serial`.

## Start / stop

```
oath ls --kind svc
oath get svc:hold
oath set svc:hold enabled=false
oath apply svc:hold
oath get svc:hold --actual
oath undo
```

Actual `state` is `stopped` | `starting` | `running` | `failed`.
