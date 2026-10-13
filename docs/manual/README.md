# Oath manual

How the system works **today**. Not a roadmap. If it is not here, do
not assume Debian, systemd, or another distro.

Oath is a Linux you administer with `oath`. The live catalog is
`/oath`. The image is **headless**: you reach it over SSH as `home`,
or on the serial console as root. There is no desktop.

| Page | What |
|------|------|
| [Using `oath`](using.md) | Verbs, safety, accounts |
| [Catalog](catalog.md) | Objects, kinds, seed profiles, overlays, devices |
| [Services](services.md) | PID 1 and `svc:*` |
| [Packages](packages.md) | Store, `/bin` links, plans, the host store cache |
| [Network and SSH](network.md) | `net:net0`, `ssh:local`, dropbear |
| [Generations](generations.md) | Apply, undo, btrfs `@gen-N`, boot menu |
| [Build and QEMU](build.md) | Kernel, image, profiles, running it |
| [Install](install.md) | `cargo make install` to a named disk; USB stick |

On a running box, start at `/oath/INDEX.md` (or `oath` with no
arguments). That file is generated from the live catalog.

## Known gaps

These are real today and scheduled to be fixed. Plan around them.

- Kernel modules load from a fixed list, not by the devices present.
- The DHCP client renews the lease, but `net:net0` actual shows only
  the first lease.
- Changes that can cut off SSH (`svc:sshd`, `ssh:local`, `net:net0`)
  need `--confirm`, but once applied nothing rolls them back for you.
- `oath new` fills in no defaults: give every required field.
- Busybox, btrfs-progs, dropbear, the OpenSSH client bits, curl,
  sgdisk, mkfs.fat and the firmware are still borrowed from Nix at
  build time. The kernel, PID 1 and `oath` are built here.
