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
| `svc:river` | `/bin/river` | enabled, `restart=always` | Patched River on `dev:card0`. Wayland socket under `/run/user/0`. |

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
