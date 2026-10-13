# Oath

A new Linux distribution: the Linux kernel and our own userspace. Not a
remix of Debian, Arch, NixOS, Alpine or Ubuntu.

**Independence. Simplicity. Openness. Courage.**

The sysadmin is an **AI agent**. A new OS has no Stack Overflow and no
training data, so the running system has to teach a model that has
never seen it. Unix is the substrate. The admin interface is not bash
folklore and not a chatbot in PID 1.

License: MIT. [LICENSE](LICENSE). Copyright (c) Joshua Kifer.

## What it is

A small number of typed **objects** in a live catalog at `/oath`. One
command, `oath`, is how humans and agents inspect and change them. PID 1
is ours; its only service config is `svc` objects. Apply snapshots the
root to a btrfs generation first, so a change can be undone.

An agent that has never seen Oath should be able to:

1. Read `/oath/INDEX.md` (or run `oath`).
2. `oath ls` · `oath schema host` · `oath get host:local`
3. `oath set host:local hostname=…` · `oath diff` · `oath apply`
4. Reboot; the name is still there. `oath undo` goes back a generation.

Kinds: `host`, `svc`, `snap`, `pkg`, `net`, `ssh`, `dev`, `plan`.

## Status (2026-10-13)

Early and not a daily driver. A review on 2026-10-13 cut the desk, the
Steam and Omarchy experiments and the Wicket host image out of the core
and set the order of work: a test harness, catalog safety, PID 1, then
a **native** base where every package is built from source by an Oath
`plan` or is the vendor's own pinned release.

What is true today:

- PID 1, the catalog, `oath`, the package store and the installer are
  ours, in Rust, statically linked against musl.
- The kernel is vanilla kernel.org source, pinned by sha256 and built by
  `image/build-linux.sh`.
- The other userspace tools in the image (busybox, btrfs-progs,
  dropbear, curl, partitioning tools) and the boot loader still come
  from nixpkgs through `image/tools.nix`. Replacing them is roadmap
  phase 5.

Focus: [CURRENT.md](CURRENT.md). Decisions: [docs/decisions.md](docs/decisions.md).
Maturity: [docs/capabilities.md](docs/capabilities.md). Manual:
[docs/manual/](docs/manual/README.md). Contributor and agent guide:
[AGENTS.md](AGENTS.md).

## Build

Needs Rust with the `x86_64-unknown-linux-musl` target, Nix (for the
temporary build tools), sudo (to loop-mount the image) and QEMU.

```sh
cargo test
nix-shell image/linux-build.nix --run 'sh image/build-linux.sh'   # the kernel, once
cargo make build                 # headless base image (sudo for the loop mount)
cargo make run                   # serial console in this terminal
cargo make up | start | ssh | stop
```

Install to a machine (wipes the disk; needs the target's SSH):

```sh
cargo make install --target user@host --disk /dev/sda --hostname name \
  --overlay hosts/name.json --confirm
```

## Repo

```
crates/oath-core   catalog, kinds, apply/undo, seed profiles
crates/oath        the oath CLI
crates/oath-init   PID 1, serial login, sudo
crates/oath-efi    UEFI boot menu
crates/oath-make   host build tool (cargo make)
image/             kernel build, pkg:cc pack, temporary nix tools
hosts/             per-machine facts and install overlays
docs/              decisions, capabilities, roadmap, specs, manual
```
