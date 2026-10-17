# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row in the same change as the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-16

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec (desired, applied, meta) on disk, status on a tmpfs rebuilt each boot (R14); schemas generated from the Rust types; generated `INDEX.md`; locked atomic writes; typed `set`; `oath new` (fills defaults) / `oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo verify log logs schema fmt build`, `--json`; bare `oath` prints the INDEX; apply returns after PID 1 converged | MCP adapter is roadmap phase 7 |
| Generations | shipped | checks before the snapshot; every outcome logged; undo restores only the touched objects' spec; lockout-class changes go on probation until `oath keep` from a new session; power alone takes no generation | — |
| Services (PID 1) | shipped | runs only applied specs; per-service logs; exit codes, backoff, `failed` with a `reason`; a cgroup per service; shutdown in reverse `wants` order; a cycle skips only its members; a main-loop panic re-executes PID 1, services untouched | — |
| Packages | partial | hash-in-path store, `/bin` farm, pins, `needs` graph, components + advisories, fetch by `url`; publish: profile packages with licence, plan and source pack; apply and `oath verify` (each boot) re-hash linked trees | RPATHs to hash paths wait for dynamic packs |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox in its own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/`; every image package, the kernel and Oath binaries included, is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap`: Clang/LLVM 23.1.3 + musl 1.2.6; `cargo make pkgs`: 27 plan products (sha256 pins, upstream OpenPGP and Sigstore signatures, SPDX licences, `pkgs/lock.json`), `pkg:llvm`, the kernel and the vendor's Rust among them; bit-identical rebuilds | Unsigned upstreams (Perl): sha256 only |
| Network | partial | `net:net0` dhcp or static; NIC renamed `net0`; a supervised DHCP client whose renewals show in actual; time from pool.ntp.org | One NIC, chosen at boot |
| SSH | shipped | dropbear server as `home`, keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` for `home` (clean env, groups dropped) | — |
| Devices | partial | modules by modalias at boot, on kernel request and on hotplug; `dev:*` discovered from sysfs (disks by bus path, `net0`…, UARTs) at boot and on change | Node ownership and mounts are not fields yet |
| Seed and overlays | shipped | profile `base`, free of test fixtures; `svc:serial` idles with no UART; `svc:screen` is a `home` shell on tty1; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, a plan built by `pkg:llvm` (7.3-rc1, Oath fragment and patches, depmod index); vermagic checked; canto runs it | — |
| Boot | partial | initrd from our PID 1, native busybox and firmware; switch_root into `@`; `oath-efi` the only loader; ESP keeps the last 5 boots; boots canto; `cargo make update` moves an installed machine to a new output (rehearsed in QEMU) | `update` not yet run on metal |
| Image build | shipped | `cargo make build` packs `pkg:oath` and the locked packages into an immutable `build/out/<id>/` with a manifest of every shipped pack; install and `esp` take its id | — |
| Install | shipped | installer ramdisk over SSH; GPT + ESP + btrfs `@`; `--overlay`; pins the host key; `--qemu` rehearsal; `--usb`; one checked output; refuses the build host; canto reinstalled unattended | — |
| Test harness | shipped | `cargo make check`: one QEMU per case, real reboots, known gaps per phase, `check-pass` per output; cases make their own fixtures; `--target`: the `metal` cases on an installed machine, undone after each | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (River, Sola) | target | Returns when River, wlroots, mesa, Sola and their deps build natively. |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. buccina still runs the trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): native desk, Steam runtime, JVM. |
