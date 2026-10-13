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

- Catalog writes are not atomic and nothing takes a lock. Two writers
  at once (`oath` and PID 1) can leave a torn file. PID 1 stops any
  service whose `desired.json` it cannot parse.
- A failed `oath apply` is not logged. `oath undo` after a failure
  reverts the previous **good** apply.
- `oath undo` restores `/oath/log` and `/oath/store` as well as the
  objects.
- PID 1 reads `desired.json`, not what was applied. A `set` without
  `apply` reaches PID 1 at the next converge or boot. A `wants` cycle
  that gets there starts no services, `svc:sshd` included.
- There are no per-service logs and no exit codes in actual.
- Nothing on the box can delete an object, and `oath set` cannot
  create a `svc`, `pkg`, or any other ordinary object. New objects
  come from the seed profile, an install overlay, `oath set
  plan:<name> --from-file` (plans only), or `oath build` (the
  product's `pkg` object).
- If `/oath/INDEX.md` is missing at boot, PID 1 seeds a fresh `base`
  catalog over `/oath`.
- `cargo make esp` mounts and writes the ESP node you name on the
  machine it runs on. Nothing stops that being the build host's own
  ESP.
- Busybox, btrfs-progs, dropbear, the OpenSSH client bits, curl,
  sgdisk, mkfs.fat and the firmware are still borrowed from Nix at
  build time. The kernel, PID 1 and `oath` are built here.
