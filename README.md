# Oath

A new Linux distribution: Linux kernel, our own userspace. Not a remix of
Debian, Arch, NixOS, Alpine, or Ubuntu.

**Independence. Simplicity. Openness. Courage.**

The sysadmin is an **AI agent**. A novel OS has no Stack Overflow and no
training data, so the running system has to teach a model that has never
seen it. Unix is the substrate. The admin interface is not bash folklore
and not a chatbot in PID 1.

License: MIT. [`LICENSE`](LICENSE). Copyright (c) Joshua Kifer.

---

## What this is

A small number of typed **objects** in a live catalog at `/oath`. One
command, `oath`, is how humans and agents inspect and change them. PID 1
is ours; its only service config is `svc` objects. Apply takes a
**btrfs** snapshot first so change is undoable.

An agent that has never seen Oath is supposed to:

1. Read `/oath/INDEX.md` (or run `oath` with no arguments).
2. `oath ls` · `oath schema host` · `oath get host:local`
3. `oath set host:local hostname=…` · `oath diff` · `oath apply`
4. Reboot. The name is still there. `oath undo` restores the previous
   generation.

If that loop works, this is a distro. A package archive without it is not.

**Kinds now:** `host`, `svc`, `snap`, `pkg`, `net`, `ssh`, `dev`.

## What this is not

A kernel project. A skin over another distro. systemd or dinit with a
new coat of paint. English as an OS API. A year-one glibc/FHS
compatibility promise.

---

## Status

Phases 0–2 are done. The **x86_64 QEMU** appliance has PID 1, catalog,
packages (store + `/bin` links, including `pkg:fetchme` wget), `net:net0`,
dropbear SSH, `dev:*` (including virtio-gpu `card0` and virtio
keyboard/mouse), and a gtk window when `DISPLAY` is set. `svc:river`
is the compositor (T21/T22). The Sola session stack is `pkg:sola` +
five `svc:*` (T23/T25) plus `sola-terminal` in that blob (T26).
Nothing is a daily driver. Metal canary (canto) installs with
`cargo make install`.

Operator manual: [`docs/manual/README.md`](docs/manual/README.md).
What next: [`CURRENT.md`](CURRENT.md).

| | |
|--|--|
| Living focus | [`CURRENT.md`](CURRENT.md) |
| Operator manual | [`docs/manual/README.md`](docs/manual/README.md) |
| Agent / contributor guide | [`AGENTS.md`](AGENTS.md) |
| Catalog freeze | [`docs/specs/2026-08-27-catalog-and-oath-surface.md`](docs/specs/2026-08-27-catalog-and-oath-surface.md) |
| Package freeze | [`docs/specs/2026-08-28-packages.md`](docs/specs/2026-08-28-packages.md) |
| Net freeze | [`docs/specs/2026-08-29-net.md`](docs/specs/2026-08-29-net.md) |
| SSH freeze | [`docs/specs/2026-08-30-ssh-and-dhcp.md`](docs/specs/2026-08-30-ssh-and-dhcp.md) |
| Devices freeze | [`docs/specs/2026-08-30-devices.md`](docs/specs/2026-08-30-devices.md) |
| Display freeze | [`docs/specs/2026-08-30-display.md`](docs/specs/2026-08-30-display.md) |
| Docs map | [`docs/README.md`](docs/README.md) |

## Build

Needs a Rust toolchain with `x86_64-unknown-linux-musl`, and (for the
image) QEMU, btrfs-progs, a Linux bzImage plus matching modules, and a
static busybox. A Nix `shell.nix` can provide the borrowed build tools;
they are not the runtime.

```sh
cargo test -p oath-core
nix-shell                 # optional: kernel, qemu, musl cc, busybox, btrfs
cargo make build          # sudo to loop-mount the disk
cargo make probe          # courage test; writes build/runs/<id>/
cargo make run            # interactive serial + run dir
cargo make run --build    # pack first (also on up / start)
cargo make up             # headless; Ctrl-C kills QEMU
cargo make start && cargo make ssh && cargo make stop
```

See [`CURRENT.md`](CURRENT.md) and [`docs/manual/qemu.md`](docs/manual/qemu.md).

## Repo

```
crates/oath-core   catalog, kinds, apply/undo
crates/oath        CLI (guest)
crates/oath-init   PID 1 + serial login
crates/oath-make   host build CLI (`cargo make`): pack image, QEMU run, probe
image/             tools.nix (borrowed kernel/busybox/qemu)
docs/              progress model, freeze, operator manual
```
