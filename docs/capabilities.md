# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row with the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-23

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec on disk, status on a tmpfs rebuilt each boot (R14); generated schemas and `INDEX.md`; locked atomic writes; typed `set`; `oath new` / `oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo verify log logs schema fmt build store mcp`, `--json` (stdout carries only the answer); bare `oath` prints the INDEX; apply returns after PID 1 converged | — |
| Agent surface | shipped | `oath mcp` over SSH: a tool per verb (writes via `sudo`), resources, `--read-only`; the log records `actor.via`; `check --agent` passes all five tasks ([specs/agent.md](specs/agent.md)) | — |
| Generations | shipped | checks before the snapshot; every outcome logged; undo restores only the touched objects' spec; lockout-class changes go on probation until `oath keep` from a new session | — |
| Services (PID 1) | shipped | runs only applied specs; per-service logs; exit codes, backoff, `failed` with a `reason`; a cgroup per service; shutdown in reverse `wants` order; survives its own panic; a service restarts when apply moves its program's package | — |
| Packages | shipped | hash-in-path store, `/bin` and `/lib` farms (R17; `lib32/` never linked, R29), pins, `needs` graph, components + advisories; fetch by `url` or `host:local.origins` (R25; default `store.oath.moonlight.pm`); `oath store` for a tree built on the machine; recipes (R23); publish uploads every locked pack with licence, plan and source; `oath verify` each boot | Oath's packs unsigned |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox in its own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/`; every image package, the kernel and Oath binaries included, is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap`: GCC 16.2 + binutils 2.47 + glibc 2.44 (R17); `cargo make pkgs`: 156 plan products (pinned and signature-checked sources, SPDX licences, `pkgs/lock.json`), i386 ones from the same scripts (R29); GNU userland (R18); bit-identical rebuilds | Perl unsigned |
| Network | partial | `net:net0` dhcp or static; NIC renamed `net0`; supervised dhcpcd, renewals in actual; LAN names in `host:local.hosts`; `dig`; chrony | One NIC, chosen at boot |
| SSH | shipped | dropbear server as `home` (login shell zsh, R27), keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` (clean env) | — |
| Devices | partial | modules by modalias (boot, request, hotplug); `dev:*` from sysfs (disks by bus path, `net0`…, UARTs) at boot and on change; `host:local.mounts` by label or UUID (boot, hotplug, apply); sound and pad nodes the seat's (R22, R30) | Node ownership is not a field |
| Seed and overlays | shipped | profile `base`, no test fixtures; `svc:screen` is a `home` shell on tty1; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, a plan built by `pkg:gcc` (7.3-rc1, Oath fragment and patches, depmod index); vermagic checked | — |
| Boot | shipped | initrd from our PID 1, bash and firmware; switch_root into `@`; `oath-efi` the only loader; ESP keeps the last 5 boots; `cargo make update` moves a machine to a new output | — |
| Image build | shipped | `cargo make build` packs `pkg:oath`, the locked packages and packs a profile pins from elsewhere (R28) into an immutable `build/out/<id>/` with a manifest | — |
| Install | shipped | installer ramdisk over SSH; GPT + ESP + btrfs `@`; `--overlay`; pins the host key; `--qemu`; `--usb`; checked outputs only | — |
| Test harness | shipped | `cargo make check`: one QEMU per case, real reboots, known gaps per phase, `check-pass` per output; `--target`: the `metal` cases on a machine; `--agent`: real-model tasks (opt-in) | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (rivulet, Sola) | partial | Sola's shell, terminal, Projects, browser, Design; shells survive session restarts; PipeWire; Noto; Vulkan (RADV); canto (R20–22). Gaps: other apps, RT priority |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. |
| Steam + Minecraft desk | partial | Steam (R29): Valve's launcher as a recipe, in a bubblewrap sandbox; the 64-bit client signed in on canto, GPU-rendered, 32-bit Mesa imported. Gaps: a 32-bit game tested, gamepads, Minecraft |
