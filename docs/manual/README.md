# Oath manual

How the system works **today**. Not a roadmap. If it is not here, do not
assume Debian, systemd, or another distro.

Oath is a Linux you administer with `oath`. The live catalog is `/oath`.
The only dogfood form is an **x86_64 QEMU** appliance (serial, SSH,
virtio-gpu window if `DISPLAY` is set).

| Page | What |
|------|------|
| [Using `oath`](using.md) | Verbs, INDEX, safety |
| [Catalog](catalog.md) | Objects, kinds, on-disk tree |
| [Services](services.md) | PID 1 and `svc:*` |
| [Packages](packages.md) | store, `/bin` links, `hello` / `fetchme` |
| [Network](network.md) | `net:net0`, static or dhcp |
| [Devices](devices.md) | `vda` / `net0` / `ttyS0` / `card0` / `kbd0` / `mouse0`; Unix floor |
| [SSH](ssh.md) | `ssh:local` keys, dropbear, no baked private key |
| [Generations](generations.md) | Apply, undo, btrfs `@gen-N` |
| [The appliance](qemu.md) | Build, run, probe, disk layout |

On a running box, start at `/oath/INDEX.md` (or `oath` with no arguments).
That file is generated from the live catalog and cannot advertise kinds
that are not there.

## What this is not

No installer, no full Sola apps (browser, mail, …). Packages: sealed
`busybox` / `btrfs` / `oath` / `dropbear` / `glibc`, plus `river`,
`sola`, `hello`, and `fetchme`. `svc:seatd` + `svc:river` is the
compositor (libinput via libudev-zero; no udevd). `svc:sola-bus` /
`sola-call` / `sola-river` / `sola-shell` is the session stack. `net:net0` + root SSH (host keys injected on
`up`/`start`). Devices include `dev:card0`, `dev:kbd0`, `dev:mouse0`.
`/proc` `/sys` `/dev` plus tmpfs and cgroup2.
