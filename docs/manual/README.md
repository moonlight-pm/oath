# Oath manual

How the system works **today**. Not a roadmap. If it is not here, do
not assume Debian, systemd, or another distro.

Oath is a Linux you administer with `oath`. The live catalog is
`/oath`. The `base` profile has no desktop: you reach it over SSH or
on the screen and keyboard as `home`, or on the serial console as root.
The optional `desk` profile adds one ([desk spec](../specs/desk.md)).

| Page | What |
|------|------|
| [Using `oath`](using.md) | Verbs, safety, disks, kernel module options, device protocols, accounts, agents (`oath mcp`) |
| [Catalog](catalog.md) | Objects, kinds, seed profiles, overlays, devices |
| [Services](services.md) | PID 1 and `svc:*` |
| [Packages](packages.md) | Store, `/bin` and `/lib` links, publishing, origins, plans, recipes, the security audit, the host store cache |
| [Cells](cells.md) | `pkg:cell` and `cell run`: a package, a `cell:<name>` or an image in a box; the `cell` kind |
| [writ](writ.md) | `pkg:writ`, the coding agent: its box, models, sessions, `writ diff`, `apply`, `undo` |
| [Network and SSH](network.md) | `net:net0`, names, time, `ssh:local`, sshd |
| [Generations](generations.md) | Apply, undo, btrfs `@gen-N`, boot menu |
| [Build and QEMU](build.md) | Toolchain, kernel, image, profiles, `cargo make check`, running it, telemetry |
| [Install](install.md) | `cargo make install` to a named disk; USB stick; `oath install` on the machine; `oath update` |

On a running box, start at `/oath/INDEX.md` (or `oath` with no
arguments). That file is generated from the live catalog.

## Known gaps

- `net:net0` is the only link. `oath new` makes no `net` object; a
  second NIC is listed as `dev:net1` and is not configured.
- The `base` image does not carry `pkg:gcc`
  ([Plans](packages.md#plans)).
- Mesa drives AMD GPUs (radeonsi, RADV) and softpipe, and no Intel
  GPU. Under QEMU `svc:sola-shell` gets no graphics context and ends
  `failed`.
- `pkg:python` is built without POSIX semaphores:
  `multiprocessing.Semaphore()` raises `ImportError`.

What a domain still lacks is in its [spec](../specs/README.md): "Known
defects" for what is built; "Target", "Will be" or "Later" for what is
not.
