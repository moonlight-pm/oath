# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row in the same change as the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-17

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec (desired, applied, meta) on disk, status on a tmpfs rebuilt each boot (R14); schemas generated from the Rust types; generated `INDEX.md`; locked atomic writes; typed `set`; `oath new` (fills defaults) / `oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo verify log logs schema fmt build mcp`, `--json` (stdout carries only the answer); bare `oath` prints the INDEX; apply returns after PID 1 converged | — |
| Agent surface | partial | `oath mcp` over SSH: a tool per verb (writes via `sudo`), INDEX as instructions, MCP 2026-07-28 and older, `--read-only`; the log records `actor.via`, `confirm`; `check --agent` ([specs/agent.md](specs/agent.md)) | Real-model check not passed yet; not on canto |
| Generations | shipped | checks before the snapshot; every outcome logged; undo restores only the touched objects' spec; lockout-class changes go on probation until `oath keep` from a new session; power alone takes no generation | — |
| Services (PID 1) | shipped | runs only applied specs; per-service logs; exit codes, backoff, `failed` with a `reason`; a cgroup per service; shutdown in reverse `wants` order; a cycle skips only its members; survives its own panic | — |
| Packages | partial | hash-in-path store, `/bin` farm, pins, `needs` graph, components + advisories, fetch by `url`; publish: profile packages with licence, plan and source pack; apply and `oath verify` (each boot) re-hash linked trees | RPATHs to hash paths wait for dynamic packs |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox in its own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/`; every image package, the kernel and Oath binaries included, is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap`: Clang/LLVM 23.1.3 + musl 1.2.6; `cargo make pkgs`: 27 plan products (sha256 pins, OpenPGP and Sigstore signatures, SPDX licences, `pkgs/lock.json`); bit-identical rebuilds | Unsigned upstreams (Perl): sha256 only |
| Network | partial | `net:net0` dhcp or static; NIC renamed `net0`; a supervised DHCP client whose renewals show in actual; time from pool.ntp.org | One NIC, chosen at boot |
| SSH | shipped | dropbear server as `home`, keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` for `home` (clean env, groups dropped) | — |
| Devices | partial | modules by modalias at boot, on kernel request and on hotplug; `dev:*` discovered from sysfs (disks by bus path, `net0`…, UARTs) at boot and on change | Node ownership and mounts are not fields yet |
| Seed and overlays | shipped | profile `base`, free of test fixtures; `svc:screen` is a `home` shell on tty1; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, a plan built by `pkg:llvm` (7.3-rc1, Oath fragment and patches, depmod index); vermagic checked; canto runs it | — |
| Boot | shipped | initrd from our PID 1, native busybox and firmware; switch_root into `@`; `oath-efi` the only loader; ESP keeps the last 5 boots; `cargo make update` moves an installed machine to a new output (canto) | — |
| Image build | shipped | `cargo make build` packs `pkg:oath` and the locked packages into an immutable `build/out/<id>/` with a manifest of every shipped pack; install and `esp` take its id | — |
| Install | shipped | installer ramdisk over SSH; GPT + ESP + btrfs `@`; `--overlay`; pins the host key; `--qemu` rehearsal; `--usb`; checked outputs only; refuses the build host | — |
| Test harness | shipped | `cargo make check`: one QEMU per case, real reboots, known gaps per phase, `check-pass` per output; cases make their own fixtures; `--target`: the `metal` cases on an installed machine, undone after each; `--agent`: real-model tasks (opt-in) | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (River, Sola) | target | Returns when River, wlroots, mesa, Sola and their deps build natively. |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. buccina still runs the trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): native desk, Steam runtime, JVM. |
