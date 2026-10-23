# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row with the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-20

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec (desired, applied, meta) on disk, status on a tmpfs rebuilt each boot (R14); schemas generated from the Rust types; generated `INDEX.md`; locked atomic writes; typed `set`; `oath new` (fills defaults) / `oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo verify log logs schema fmt build store mcp`, `--json` (stdout carries only the answer); bare `oath` prints the INDEX; apply returns after PID 1 converged | — |
| Agent surface | shipped | `oath mcp` over SSH: a tool per verb (writes via `sudo`), resources, INDEX as instructions, `--read-only`; the log records `actor.via`; `check --agent`: haiku passes all five tasks ([specs/agent.md](specs/agent.md)) | — |
| Generations | shipped | checks before the snapshot; every outcome logged; undo restores only the touched objects' spec; lockout-class changes go on probation until `oath keep` from a new session | — |
| Services (PID 1) | shipped | runs only applied specs; per-service logs; exit codes, backoff, `failed` with a `reason`; a cgroup per service; shutdown in reverse `wants` order; a cycle skips only its members; survives its own panic; a service restarts when apply moves its program's package | — |
| Packages | shipped | hash-in-path store, `/bin` and `/lib` farms (R17), pins, `needs` graph, components + advisories; fetch by `url` or `host:local.origins` (R25; default `store.oath.moonlight.pm`); `oath store` for a tree built on the machine; recipes (R23); publish uploads every locked pack with licence, plan and source; `oath verify` each boot | Oath's packs unsigned |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox in its own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/`; every image package, the kernel and Oath binaries included, is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap`: GCC 16.2 + binutils 2.47 + glibc 2.44 (R17); `cargo make pkgs`: 134 plan products (pinned and signature-checked sources, SPDX licences, `pkgs/lock.json`); GNU userland (R18); bit-identical rebuilds; compilers on the desk | Perl unsigned |
| Network | partial | `net:net0` dhcp or static; NIC renamed `net0`; supervised dhcpcd, renewals in actual; LAN names in `host:local.hosts`; `dig`; chrony | One NIC, chosen at boot |
| SSH | shipped | dropbear server as `home` (login shell zsh, R27), keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` (clean env) | — |
| Devices | partial | modules by modalias at boot, on kernel request and on hotplug; `dev:*` discovered from sysfs (disks by bus path, `net0`…, UARTs) at boot and on change | Node ownership and mounts are not fields yet |
| Seed and overlays | shipped | profile `base`, no test fixtures; `svc:screen` is a `home` shell on tty1; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, a plan built by `pkg:gcc` (7.3-rc1, Oath fragment and patches, depmod index); vermagic checked | — |
| Boot | shipped | initrd from our PID 1, bash and firmware; switch_root into `@`; `oath-efi` the only loader; ESP keeps the last 5 boots; `cargo make update` moves a machine to a new output | — |
| Image build | shipped | `cargo make build` packs `pkg:oath`, the locked packages and packs a profile pins from elsewhere (R28) into an immutable `build/out/<id>/` with a manifest | — |
| Install | shipped | installer ramdisk over SSH; GPT + ESP + btrfs `@`; `--overlay`; pins the host key; `--qemu`; `--usb`; checked outputs only | — |
| Test harness | shipped | `cargo make check`: one QEMU per case, real reboots, known gaps per phase, `check-pass` per output; `--target`: the `metal` cases on a machine, undone after each; `--agent`: real-model tasks (opt-in) | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (rivulet, Sola) | partial | Sola's shell, terminal, Projects, browser, Design; shells survive session restarts; PipeWire; Noto; Vulkan (RADV); canto (R20–22). Gaps: other apps, RT priority |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. buccina still runs the trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): native desk, Steam runtime, JVM. |
