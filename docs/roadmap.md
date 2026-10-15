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

## 2. Harness — done (2026-10-13)

`cargo make check`: headless QEMU running the README loop and the
known failure cases over SSH from a data file. Installs are gated on it.

## 3. Catalog safety — done (2026-10-13)

Built: locked atomic writes (R4), typed `set`, id grammar, validation
before the snapshot, every outcome logged (`ok` / `error` /
`refused`), a stack undo that restores only the touched objects' spec
(never log, store or host keys), `oath new` / `oath rm` through apply,
`--confirm` for changes that can cut off SSH, PID 1 running only
applied specs, no reseed of an existing catalog.

Not done, carried forward: schemas generated from the Rust types,
probation apply with automatic rollback, a physical spec/status
directory split (undo already touches spec only), a store integrity
check at boot.

## 4. PID 1 — done (2026-10-13)

Built: per-service logs and `oath logs`, exit codes and a real restart
count, backoff into a `failed` state, one cgroup per service
(`keep_children` for sshd), an orderly shutdown on `reboot` / `halt` /
`poweroff` and `oath` power changes, switch_root, a panic hook with
`panic=10` on metal, a fallback shell that reaps, a supervised DHCP
client that renews the lease, and `oath apply` waiting for PID 1 to
converge. All 25 harness cases pass.

Not done, carried forward: module loading by modalias (with metal, in
phase 6), shutdown in reverse `wants` order, the cycle reason in actual,
surviving a supervisor panic.

## 5. Native base — done (2026-10-14)

Built: Clang/LLVM 23.1.3 on musl 1.2.6 as the system toolchain (R11).
`cargo make bootstrap` builds stage 0 with the build host's gcc (never
ships) and stage 1 with stage 0: a static `pkg:llvm` with its own
sysroot, busybox and make. `cargo make pkgs` turns `pkgs/<name>/`
(sha256-pinned sources, SPDX licence, `build.sh`) into pinned plans and
builds them in the sandbox; `pkgs/lock.json` records each product. 15
packages by plan: busybox, make, pkgconf, zlib, zstd, lzo, util-linux,
btrfs-progs, dropbear, OpenSSH, mbedTLS, curl, dosfstools, kexec-tools
and pinned firmware. Rebuilding with the stage-2 tools gives
bit-identical products. The image takes every userspace package from
`lock.json`; `image/tools.nix` and systemd-boot are gone (oath-efi is
the only loader); the installer uses sfdisk, wipefs and native mkfs.
27 harness cases and the `install --qemu` rehearsal pass.

Not done, carried forward: the kernel built by a plan with our compiler,
the Rust toolchain as a pinned vendor pack, upstream signature checks
(sources are sha256-pinned), a publish allowlist that reads the
licences in `lock.json`, immutable build outputs with a manifest.

## 6. Reinstall — active

Wipe canto and buccina, install the native base from a clean build,
run the harness against both over SSH, load modules by modalias.

Built: `cargo make check --target` runs the `metal` cases on an
installed machine and undoes back after each; install works through
any login shell and pins the new host key; the seed lost QEMU's
`dev:vda`. canto was reinstalled unattended on 2026-10-14 and passes
every metal case. PID 1 loads modules by modalias and is the kernel's
modprobe helper; every built module ships with depmod's index. canto
loads exactly its own drivers and passes all 24 metal cases
(2026-10-15). `svc:time` keeps the clock from pool.ntp.org (R12). Left: buccina (parked by Joshua).

## 7. Agent surface — planned

MCP and agent tooling over `oath --json` (the old "Phase 5").

## 8. Native desk — planned

River, wlroots, mesa, input, audio, fonts and Sola built natively.
Sola-compat rules from the old fork workflow still apply.

## 9. Steam + Minecraft desk — planned

A desk for Joshua's daughter that launches Steam and runs Minecraft.
Needs phase 8, Steam's runtime (vendor release) and a JVM.
