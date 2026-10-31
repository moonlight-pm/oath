# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row with the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-29

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec on disk, status on a tmpfs rebuilt each boot (R14); generated schemas and `INDEX.md`; locked atomic writes; typed `set`; `oath new` / `oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo verify log logs schema fmt build store mcp`, `--json`; bare `oath` prints the INDEX; apply waits for PID 1 | — |
| Agent surface | shipped | `oath mcp` over SSH: a tool per verb (writes via `sudo`), resources, `--read-only`; the log records `actor.via`; `check --agent` passes five tasks | — |
| Generations | shipped | checks before the snapshot; outcomes logged; undo restores the touched objects' spec; lockout-class changes stay on probation until `oath keep` | — |
| Services (PID 1) | shipped | runs applied specs; per-service logs; exit codes, backoff, `failed` with a `reason`; a cgroup each; shutdown in reverse `wants` order; survives its own panic; restarts a service its package moves (an update spares the seat's) | — |
| Packages | shipped | hash-in-path store, `/bin` and `/lib` farms (R17, R29), pins, `needs`, advisories; fetch from `host:local.origins` (R25); recipes (R23, R35); publish with licence, plan, source; `oath verify` each boot | Oath's packs unsigned |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox, own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/`; every image package is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap` (R32): GCC 16.2, binutils 2.47, glibc 2.44 (R17); `cargo make pkgs`: 161 plan products (signed sources, SPDX), i386 too (R29); GNU userland (R18); bit-identical; CEF by Chromium's Clang (R40) | Perl unsigned; Python's multiprocessing has no semaphores |
| Network | partial | `net:net0` dhcp or static; NIC renamed `net0`; supervised dhcpcd; LAN names in `host:local.hosts`; `dig`; chrony | One NIC, chosen at boot |
| SSH | shipped | OpenSSH's sshd (R36) as `home` (zsh, R27), keys from `ssh:local`, root denied, `host:local` env; `sftp-server`; daemons drop to `void`; `sudo` | — |
| Login and seat | partial | `svc:login` (R42): `home`'s password, Argon2id; `oath password`; `host:local.autologin`; the seat's turns (`oath desk`), the console's prompt; on canto | No password on metal yet; `sudo` asks none |
| Devices | partial | modules by modalias (boot, request, hotplug); `dev:*` from sysfs; `host:local.mounts` by label or UUID (boot, hotplug, apply), `/home` at boot (R45); module options; sound, pads and `/dev/kvm` `home`'s (R22, R30, R33) | Node ownership is not a field |
| Seed and overlays | shipped | profile `base`; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, built by `pkg:gcc` (7.3; novus's drivers, KVM, nftables); vermagic checked; `pkg:linux-build` for outside modules (NVIDIA's on novus's card, R34) | — |
| Boot | shipped | initrd from our PID 1, bash and firmware; switch_root into `@`; `oath-efi` the only loader; ESP keeps the last 5 boots; `oath update` pulls the newest signed output (R38) and builds recipes | No `--reboot`: an older PID 1 restarts the desk |
| Image build | shipped | `cargo make build` packs `pkg:oath`, the locked packages and packs a profile pins from elsewhere (R28) into an immutable `build/out/<id>/` with a manifest | — |
| Install | shipped | installer ramdisk; GPT, ESP, `@` by UUID; `--overlay`; pins the host key; `--qemu`, `--usb`, `--local`; checked outputs only; `oath install` (R43), `@home` apart; records its release (R45); novus | AMI: `UEFI OS` returns; Oath not first |
| Test harness | shipped | `cargo make check`: a QEMU per case, real reboots, known gaps per phase, `check-pass` per output; `--target`: `metal` cases on a machine; `--agent`: real-model tasks | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (rivulet, Sola) | partial | Sola's apps; shells survive restarts; PipeWire, realtime, BlueZ; Noto; `wl-clipboard`; libglvnd, RADV or NVIDIA (R34); H.264, AAC (R40); shortcut inhibiting (R44). Gaps: other apps; Intel GPUs; the KVM's capture, pairing (R44) |
| Wicket host | target | Left core (R1); Wicket ships its own packages. |
| Steam + Minecraft desk | partial | Steam (R29): Valve's launcher, a recipe, sandboxed; the 64-bit client on canto. Gaps: no 32-bit game or pad tried; Minecraft |
