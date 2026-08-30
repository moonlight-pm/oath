# Oath manual

How the system works **today**. Not a roadmap. If it is not here, do not
assume Debian, systemd, or another distro.

Oath is a Linux you administer with `oath`. The live catalog is `/oath`.
The only dogfood form is an **x86_64 QEMU** appliance on a serial console.

| Page | What |
|------|------|
| [Using `oath`](using.md) | Verbs, INDEX, safety |
| [Catalog](catalog.md) | Objects, kinds, on-disk tree |
| [Services](services.md) | PID 1 and `svc:*` |
| [Packages](packages.md) | `pkg:hello`, store, `/bin` links |
| [Network](network.md) | `net:net0`, static IPv4 |
| [Generations](generations.md) | Apply, undo, btrfs `@gen-N` |
| [The appliance](qemu.md) | Build, run, probe, disk layout |

On a running box, start at `/oath/INDEX.md` (or `oath` with no arguments).
That file is generated from the live catalog and cannot advertise kinds
that are not there.

## What this is not

No installer, no SSH, no desktop, no glibc-as-the-OS. Packages: sealed
`busybox` / `btrfs` / `oath`, plus canary `pkg:hello`. One static
link: `net:net0`.
