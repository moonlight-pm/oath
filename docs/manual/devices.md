# Devices

Hardware inventory is `dev:*` objects. `/dev` is nodes from **devtmpfs**,
not the admin UI. There is no udevd. `/proc` and `/sys` are mounted at
boot (kernel views, not catalog kinds).

## What ships

| Id | Class | Node |
|----|-------|------|
| `dev:vda` | block | `/dev/vda` (root disk) |
| `dev:net0` | net | `/sys/class/net/net0` |
| `dev:ttyS0` | tty | `/dev/ttyS0` |
| `dev:card0` | drm | `/dev/dri/card0` (virtio-gpu) |
| `dev:kbd0` | input | `/dev/input/event*` (virtio keyboard; node from sysfs name) |
| `dev:mouse0` | input | `/dev/input/event*` (virtio mouse; node from sysfs name) |

Not removable. `oath set dev:vda present=false` is refused.

`net:net0` is still the *link*. `dev:net0` is the *NIC*.

```
oath ls --kind dev
oath get dev:vda
```

## Unix floor

PID 1 also mounts (not catalog objects):

- tmpfs `/tmp`, `/dev/shm`, `/run`
- cgroup2 `/sys/fs/cgroup`
- `/oath/run` stays on disk (generations + init.sock)

`/etc/hosts` has `localhost` and the hostname (updated with `host:local`).
QEMU has virtio-rng.

ALSA nodes (`/dev/snd/*`) are **not** `dev:*` objects. On metal, PID 1
loads HDA modules after KMS and chowns `/dev/snd` `0660` root:`home`
the same way as DRM/evdev. No udevd.
