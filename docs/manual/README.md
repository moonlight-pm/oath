# Oath manual

How the system works **today**. Not a roadmap. If it is not here, do not
assume Debian, systemd, or another distro.

Oath is a Linux you administer with `oath`. The live catalog is `/oath`.
Dogfood: **x86_64 QEMU** appliance (serial, SSH, virtio-gpu window if
`DISPLAY` is set) and **canto** (metal, SSH + Sola as `home`).

| Page | What |
|------|------|
| [Using `oath`](using.md) | Verbs, INDEX, safety |
| [Catalog](catalog.md) | Objects, kinds, on-disk tree |
| [Services](services.md) | PID 1 and `svc:*` |
| [Packages](packages.md) | store, `/bin` links, `hello` / `fetchme` |
| [Network](network.md) | `net:net0`, static or dhcp |
| [Devices](devices.md) | `vda` / `net0` / `ttyS0` / `card0` / `kbd0` / `mouse0`; Unix floor |
| [SSH](ssh.md) | `ssh:local` keys, dropbear, scp/sftp, no baked private key |
| [Generations](generations.md) | Apply, undo, btrfs `@gen-N` |
| [The appliance](qemu.md) | Build, run, probe, disk layout |
| [Install](install.md) | `cargo make install` to a named disk; USB stick |

Off-box backup (`svc:backup`) is in [Services](services.md). System
clock is UTC; 04:00 Mountain is the backup schedule only.

On a running box, start at `/oath/INDEX.md` (or `oath` with no arguments).
That file is generated from the live catalog and cannot advertise kinds
that are not there.

## What this is not

Sola terminal, browser, workspaces (`solactl`), and arcade are packed;
other kit apps (mail, wrapper, spotify, …) are not. Steam/gamescope/
Xwayland are separate packages. Packages: sealed
`busybox` / `btrfs` / `oath` / `dropbear` / `glibc`, plus `river`,
`sola`, `grok`, `git`, `curl`, `pipewire`, `thoxa`, `cc`, `rustc`,
`cmake`, `pkg-config`, `bash`, `xwayland`, `gamescope`, `steam`,
`hello`, and `fetchme`.
`svc:seatd` + `svc:river` is the compositor (libinput via libudev-zero;
no udevd). `svc:sola-bus` / `sola-call` / `sola-river` / `sola-shell` /
`sola-session` is the session stack. Audio is `pkg:pipewire` +
`svc:pipewire` / `wireplumber` / `pipewire-pulse` (as `home`). `net:net0`
+ SSH as `home` (host keys injected on `up`/`start`). Devices include
`dev:card0`, `dev:kbd0`, `dev:mouse0`. `/proc` `/sys` `/dev` plus tmpfs
and cgroup2.
