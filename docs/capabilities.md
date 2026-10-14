# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row in the same change as the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-14 (nothing booted on metal since the 2026-10-13
cut)

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | partial | `kind:name` objects with desired/actual/meta; JSON Schema per kind; generated `INDEX.md`; locked atomic writes; typed `set`; id grammar; `oath new` / `oath rm` (svc, pkg) | Schemas are hand-written, not generated from the types; `new` fills no defaults |
| `oath` CLI | shipped | `ls get set new rm diff apply undo log logs schema fmt build`, `--json`; bare `oath` prints the INDEX; apply returns after PID 1 converged | MCP adapter is roadmap phase 7 |
| Generations | partial | checks before the snapshot (`refused` changes nothing); every outcome logged; undo is a stack that restores only the touched objects' spec; survives a catalog reboot | Lockout-class changes need `--confirm` but nothing auto-reverts them; a catalog reboot takes a generation |
| Services (PID 1) | partial | runs only applied specs; per-service logs (`oath logs`); exit codes, restart count, backoff, `failed`; a cgroup per service (`keep_children` for sshd); orderly shutdown on `reboot`/`poweroff`; a wants cycle skips only its members | Modules load from a fixed list, not by modalias; shutdown is not in reverse `wants` order |
| Packages | partial | hash-in-path store, `/bin` farm, pins, `needs` graph, description/home, `components.json` + advisories, fetch by `url` | Store integrity is assumed, not verified at boot; publish has no allowlist (`pkgs/lock.json` carries licences for one) |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; sandbox with no network and nothing from the build host; cache keyed on plan and inputs; every C package in the image is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap`: Clang/LLVM 23.1.3 + musl 1.2.6 from source; `cargo make pkgs`: 15 packages from `pkgs/` (pinned sources, SPDX licences, `pkgs/lock.json`); rebuilds are bit-identical | `pkg:llvm` comes from the bootstrap script, not a plan; the Oath crates use the host's rustc and Rust's bundled musl; upstream signatures not verified |
| Network | partial | `net:net0` dhcp (default) or static; NIC renamed to `net0`; a supervised DHCP client renews the lease | `actual.lease` shows the first lease; one NIC |
| SSH | shipped | dropbear server as `home`, keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` for `home` (clean env, groups dropped) | — |
| Devices | partial | `dev:vda`, `dev:net0`, `dev:ttyS0` inventory | Hard-coded names; modules load from a fixed list |
| Seed and overlays | shipped | profile `base` (`crates/oath-core/seed/`); profiles extend each other; `oath seed --overlay`; `install --overlay` | A `pkg` an overlay turns on is linked by the first `oath apply` |
| Kernel | shipped | one kernel from `image/build-linux.sh` (7.3-rc1, sha256-pinned, Oath fragment and patches); pack refuses a release/vermagic mismatch | Built by hand with the host's gcc before `cargo make build`; not yet a plan |
| Boot | partial | initrd from our PID 1 + native busybox and firmware; switch_root into `@`; `oath-efi` is the only loader; ESP keeps current + last 5 boots; panic hook and `panic=10`; `cargo make esp` runs only on an Oath machine | Not on metal since the cut |
| Image build | partial | `cargo make build` packs every userspace package from `pkgs/lock.json` (25 MB qcow2); Rust links with rust-lld; QEMU and OVMF come from `image/host.nix` (host only) | Output is not immutable or manifest-checked |
| Install | partial | installer ramdisk over SSH with native sfdisk, wipefs, mkfs.fat, mkfs.btrfs, kexec; GPT + ESP + btrfs `@`; `--overlay`; `--qemu` rehearsal (passes); `--usb` stick; refuses an image `cargo make check` has not passed | Copies the mutable `build/stage`, not a manifest-checked output |
| Test harness | shipped | `cargo make check`: one QEMU per case from `check/cases.toml`, real reboots, known gaps marked per phase, `build/check-pass` stamp | 27 cases, all pass |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (River, Sola) | target | Returns when River, wlroots, mesa, Sola and their deps build natively. canto still runs the 2026-09 desk image. |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. buccina still runs the trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): native desk, Steam runtime, JVM. |
