# Roadmap

Phase-level status. Feature detail is in [capabilities.md](capabilities.md);
the current slice is in [CURRENT.md](../CURRENT.md). The order below
comes from the 2026-10-13 review ([decisions.md](decisions.md)).

**Status:** done · active · next · planned

## Done before the review (2026-08-27 → 2026-09-30)

A QEMU appliance and then canto booted our PID 1 and catalog: hostname,
services, packages, network, SSH and devices as objects; btrfs
generations with undo; hash-identified packages; `plan` builds; an
installer; an ESP boot menu. A River/Sola desk, a Steam runtime, an
Omarchy session and a Wicket host image were layered on top using
relocated Nix packages. The review found the core unsafe in places and
the payload tangled into it.

## 1. Cut — done (2026-10-13)

Remove the desk, T37, T39, Wicket, the probe and the diary docs. Seed
as data with profiles and host overlays. One own-built
kernel with a vermagic check. `esp` refuses non-Oath hosts.

## 2. Harness — active

`cargo make check`: headless QEMU running the README loop and the
known failure cases over SSH from a data file. Installs are gated on it.

## 3. Catalog safety — planned

Locked atomic writes (R4), spec/status/log/store split, validate before
snapshot, log every outcome, undo restores spec only, `oath new` /
`oath rm`, typed `set`, schemas generated from the Rust types, id
grammar, probation apply for lockout-class changes.

## 4. PID 1 — planned

Per-service log ring and `oath logs`, exit codes, backoff, failed
state, cgroups, shutdown, `switch_root`, panic hook, cycle isolation,
DHCP as a service, modalias loading.

## 5. Native base — planned

Bootstrap compiler, then busybox, btrfs-progs, dropbear, curl, sgdisk,
dosfstools, firmware and the boot loader as plans. Retire
`image/tools.nix` for anything that ships. Immutable build outputs
with a manifest; publish allowlist with a license per package.

## 6. Reinstall — planned

Wipe canto and buccina, install the native base from a clean build,
run the harness against both over SSH.

## 7. Agent surface — planned

MCP and agent tooling over `oath --json` (the old "Phase 5").

## 8. Native desk — planned

River, wlroots, mesa, input, audio, fonts and Sola built natively.
Sola-compat rules from the old fork workflow still apply.

## 9. Steam + Minecraft desk — planned

A desk for Joshua's daughter that launches Steam and runs Minecraft.
Needs phase 8, Steam's runtime (vendor release) and a JVM.
