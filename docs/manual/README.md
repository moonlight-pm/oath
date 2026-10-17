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

None in shipped behavior today. What is still design lives under
"Target" in each [spec](../specs/README.md).
