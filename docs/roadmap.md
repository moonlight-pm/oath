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

Since: probation apply with automatic rollback, the store integrity
check, schemas generated from the Rust types, and the spec/status split
(R14, status on a tmpfs) (2026-10-16).

## 4. PID 1 — done (2026-10-13)

Built: per-service logs and `oath logs`, exit codes and a real restart
count, backoff into a `failed` state, one cgroup per service
(`keep_children` for sshd), an orderly shutdown on `reboot` / `halt` /
`poweroff` and `oath` power changes, switch_root, a panic hook with
`panic=10` on metal, a fallback shell that reaps, a supervised DHCP
client that renews the lease, and `oath apply` waiting for PID 1 to
converge. All 25 harness cases pass.

Since: module loading by modalias (phase 6), the cycle reason in
actual, shutdown in reverse `wants` order, and surviving a supervisor
panic (2026-10-16).

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

Since: the kernel is a plan built by the system toolchain
(2026-10-15), and the Oath binaries are `pkg:oath`, built by the
vendor's Rust pack `pkg:rust` (2026-10-16); the toolchain became GCC
and glibc on 2026-10-17 (phase 8, R17). Sources
are checked against upstream OpenPGP signatures by keys pinned in
`pkgs/keys/`, and Python's against its Sigstore bundle (2026-10-16).

`cargo make publish` carries the packages a seed profile names, each
with its licence (since 2026-10-19 every locked pack too, to the
default origin, R25), and `cargo make build` writes immutable outputs
(`build/out/<id>/`, with a manifest of every shipped pack) that install
and `esp` take by id (2026-10-16).

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
(2026-10-15). `svc:time` keeps the clock from pool.ntp.org (R12).
Devices are discovered and hot-plugged, and `cargo make update` moves
an installed machine to a new output without a reinstall
(2026-10-16). Left: buccina (parked by Joshua).

## 7. Agent surface — done (2026-10-18)

MCP and agent tooling over `oath --json` (the old "Phase 5"). Decided
2026-10-17 (R16), spec in [specs/agent.md](specs/agent.md): `oath mcp`
on each machine over SSH, one tool per verb, and an opt-in check where
a real model that has never seen Oath is given only that server and a
task. Done when that check passes in QEMU and on canto.

Built (2026-10-17): `oath mcp` in both MCP eras with every verb as a
tool, `--read-only`, `actor.via` and `confirm` in the apply log, and
`cargo make check --agent` with the five tasks of `check/agent.toml`.
Done 2026-10-18: haiku passed all five tasks on canto (output
`e3a11645f14c`) and in QEMU (output `218fd2e1e03b`), and canto runs
`218fd2e1e03b`.

## 8. Native desk — active

River, wlroots, mesa, input, audio and fonts built natively; Sola
packages itself, and the `desk` profile pins its packs (R28).

Slice 1 (R17, R18), built 2026-10-17: the toolchain is GCC 16.2,
binutils 2.47 and glibc 2.44, programs link dynamically through a
`/lib` farm, and Clang/LLVM, musl and busybox are gone; the base is the
GNU userland with util-linux, iproute2, dhcpcd, chrony and vim
([specs/builds.md](specs/builds.md)). canto was reinstalled with it on
2026-10-18 and passes every metal case. River becomes rivulet, a C port
on wlroots and a hard fork without Zig (R19, 2026-10-18).

Slice 2, the desk: rivulet and its 41 packages (Mesa with ACO and GLX,
wlroots, libinput, Xwayland and the X libraries) build natively, and
the `desk` profile (seatd, R20) runs rivulet and Sola's session, terminal
and browser on canto (R21; 2026-10-18; [specs/desk.md](specs/desk.md)).

## 9. Steam + Minecraft desk — planned

A desk for Joshua's daughter that launches Steam and runs Minecraft.
Needs phase 8, Steam's runtime (vendor release) and a JVM.
