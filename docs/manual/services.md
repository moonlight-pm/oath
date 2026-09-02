# Services

PID 1 is **oath-init** (`/usr/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/usr/lib/oath/serial-login` | enabled, `restart=always` | Root shell on serial (`ttyS0` / `hvc0`). It does not take the graphical VT; that stays on the boot mark. Do not disable it unless you have another console. |
| `svc:hold` | `/bin/sleep 86400000` | enabled, `restart=always` | Harmless sleeper for start/stop. |
| `svc:sshd` | dropbear | enabled, `restart=always` | Keys in `ssh:local`. Password off. |
| `svc:seatd` | `/bin/seatd` | enabled, `restart=always` | Seat for DRM. `svc:river` wants this. |
| `svc:river` | `/usr/lib/oath/run-compositor` | enabled, `restart=always` | Wrapper around `/bin/river`: picks the DRM card with a connected connector (dual-GPU). Pixman. libinput via libudev-zero. |
| `svc:sola-bus` | `/bin/sola-bus` | enabled, `restart=on-failure` | Sola IPC bus. Socket `/run/user/0/sola-bus`. |
| `svc:sola-call` | `/bin/sola-call` | enabled, `restart=on-failure` | Sola call host. Socket `/run/user/0/sola-call`. |
| `svc:sola-river` | `/bin/sola-river` | enabled, `restart=on-failure` | Bridge (bus ↔ Wayland). Wants `svc:river` + bus + call. Not the compositor. |
| `svc:sola-shell` | `/bin/sola-shell` | enabled, `restart=on-failure` | Iced menubar / launcher / window menu / Super+K shortcuts. Wants river + bus + call + the bridge. wgpu/gl; llvmpipe only on virtio KMS. |
| `svc:sola-session` | `/bin/sola-session` | enabled, `restart=on-failure` | LaunchApp / CloseApp owner. Wants bus + call. Direct spawn (no systemd). Kit apps in `pkg:sola`: `/bin/sola-terminal` (tmux), `/bin/sola-browser` (CEF), `/bin/sola-workspaces` (`solactl` helper). |

**Quit Sola** (flower menu) broadcasts shutdown and the session processes
exit 0. PID 1 does **not** restart them (`on-failure` only). River the
compositor, serial, and SSH stay. Reboot or `oath apply` (while they
remain `enabled=true`) starts the session again. To leave it off across
reboot: `oath set svc:sola-shell enabled=false` (and bus, call, bridge,
session) then `oath apply`.

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
