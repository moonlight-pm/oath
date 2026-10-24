# Scope: what Oath core is

Oath core is the catalog at `/oath`, the `oath` CLI, PID 1, the package
store, the plan builder, the one kernel, and the tools that pack and
install an image. Everything else is a payload: software that runs on
Oath and is shipped as packages. Payloads do not live in the core
crates, the seed, or the installer.

**Status:** built. The 2026-10-13 cut removed the desk, Steam, Omarchy
and Wicket from core.

Replaces: the scope parts of T22–T31, T36, T37, T39, T40.

## Core

- **Catalog.** Typed objects named `kind:name`. One admin surface,
  `oath`, used the same way by humans and agents. See
  [catalog.md](catalog.md).
- **PID 1.** Ours. `svc` objects are its only configuration. See
  [services.md](services.md).
- **Packages and builds.** A content-addressed store and hashed plan
  files. See [packages.md](packages.md) and [builds.md](builds.md).
- **Boot and install.** One kernel, the `oath-efi` menu, the installer.
  See [boot-and-install.md](boot-and-install.md).
- **Reaching the box.** `net`, `ssh`, the `home` account. See
  [network-and-ssh.md](network-and-ssh.md) and [devices.md](devices.md).

Principles: independence, simplicity, openness, courage. Oath is not a
remix of Debian, Arch, NixOS, Alpine or Ubuntu. Unix is the substrate;
the catalog is the admin interface.

The C library is glibc, and the toolchain GCC and binutils (R17).
Programs load glibc through `/lib/ld-linux-x86-64.so.2`; Oath's own
binaries are static. One process never loads two libcs.

## Payloads

A payload is anything an owner chooses to run: a desktop, a game
launcher, a web control plane. The rules:

- A payload ships as packages and services in the catalog. It does not
  add code paths to `oath-core`, PID 1, `oath-make`, or the seed.
- When a payload needs something core cannot express, core gains a
  **generic** field, not a payload name. Examples of fields a payload
  may justify later: a service that waits for a mount, extra btrfs
  subvolumes declared in the catalog, a toggle for root SSH, an install
  overlay. Core never tests for a product name.
- Payloads wait until the package standard is mature. Then each one
  bundles its own packages as native packages (built from source by a
  plan, or a vendor release pinned by hash).
- Machine-specific facts (hostname, timezone, a NAS address) are host
  overlays applied at install time, not seed entries.

### Payloads that left core on 2026-10-13

| Payload | Was | Where it is now |
|---------|-----|-----------------|
| Desk (River, Sola, PipeWire, BlueZ) | Nix closures relocated into the store | Returns as native packages after the native base |
| Omarchy session (Hyprland, Quickshell) | T39 | Deleted. Not planned. |
| Steam, gamescope, mesa, Xwayland (T37) and sola-arcade | T37 | Deleted. See the requirement below. |
| Wicket image (wicketd, Caddy, Postgres) | An image flavor wired into core | The Wicket project owns its packages and overlay |
| Grok, Thoxa | Personal tools in the seed | Packages an owner may add |

The last commit with all of them is tagged `t37-t39-final`.

## Requirement: a Steam and Minecraft desk

One of the first real uses of Oath is a desk for the owner's daughter
that launches Steam and runs Minecraft. Steam is being brought up on
canto since 2026-10-23 ([steam.md](steam.md), R29); Minecraft is not
started.

What it will need, as packages:

- A graphical session (compositor, input, audio) built natively.
- Mesa, 64-bit and 32-bit, built from source.
- `pkg:glibc` (64-bit and 32-bit) as a runtime object for Steam.
- Xwayland or an X server for Steam's client.
- Steam itself, the vendor's binary release pinned by hash.
- Minecraft Java Edition: a JVM (OpenJDK built from source, or a vendor
  release pinned by hash), OpenGL from mesa, and a launcher.

sola-arcade and the gamescope nest are not part of it.

## Not core, ever

- A recipe language, a second package database, or another distro's
  package set.
- A chatbot in PID 1. The agent is a client of `oath`, like a human.
- systemd, dinit, or another init with a skin.
- Editing `/etc` as the way to configure the system. Files there are
  side effects of converging catalog objects.
