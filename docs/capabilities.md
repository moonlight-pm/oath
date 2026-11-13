# Capabilities

What exists and what is missing. Design: [specs/](specs/); priority:
[CURRENT.md](../CURRENT.md). Under 800 words; update a row with the code.

**Status:** `shipped` works and is tested · `partial` works with
gaps · `target` designed, not built.

**As of:** 2026-11-11

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | shipped | `kind:name` objects: spec on disk, status on a tmpfs rebuilt each boot (R14); generated schemas and `INDEX.md`; typed `set`; `oath new`/`oath rm` | — |
| `oath` CLI | shipped | `ls get set new rm add diff apply keep undo update verify audit reap log logs schema fmt build store publish install password desk wake mcp`, `--json`; bare `oath` prints the INDEX | — |
| Agent surface | shipped | `oath mcp` over SSH: a tool per verb (writes via `sudo`), `--read-only`; `check --agent` passes five tasks | — |
| Generations | shipped | undo restores the touched objects' spec; lockout-class changes stay on probation until `oath keep`; the newest five and those since boot kept (R62) | — |
| Services (PID 1) | shipped | runs applied specs; backoff, `failed` with a `reason`; a cgroup each; survives a panic; re-execs on update (R73); restarts a service its package moves; cron `schedule` (R48) | — |
| Packages | shipped | hash-in-path store, `/bin`, `/lib` farms (R17, R29), pins, `needs`; origins (R25); recipes (R23, R35); npm programs (R46); `oath verify`; `source`, overlays (R47); CVE audit (R48, R61); `oath reap` (R62); `category` (R63); heads, `publish`, `update --record`, scoped keys, library-checked needs (R72); `oath add`, `ls --origin` (R81) | packaged services (R72); npm, GitHub signatures unchecked |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox, own PID namespace; cache keyed on plan and inputs; logs in `/oath/log/plan/` | — |
| Toolchain and native packages | partial | `cargo make bootstrap` (R32): GCC 16.2, binutils 2.47, glibc 2.44 (R17); `cargo make pkgs`: 211 products, i386 (R29); GNU userland (R18); bit-identical; CEF (R40); Go, servers, `aws`, Bun, FFmpeg (R49, R82), `route`, `cell` (R59, R76/R78); roots (R50), tags (R54), intake (R57) | Perl unsigned; Python lacks semaphores |
| Network | partial | `net:net0` dhcp or static (`dns`, R83); supervised dhcpcd; LAN names (`host:local.hosts`); `dig`; chrony; Wake-on-LAN (R84) | One NIC, chosen at boot |
| SSH | shipped | OpenSSH's sshd (R36) as `home` (zsh, R27), keys from `ssh:local`, root denied, `host:local` env; `sftp-server`; daemons drop to `void`; `sudo` | — |
| Login and seat | partial | `svc:login` (R42): `home`'s password, Argon2id; `oath password`; `host:local.autologin`; the seat's turns (`oath desk`) | No password on metal yet; `sudo` asks none |
| Devices | partial | modules by modalias, static nodes, package firmware; `dev:*` from sysfs; `host:local.mounts`, `/home` (R45); sound, pads, `/dev/kvm`, `seat_hid` (R22, R30, R33, R71); `power=sleep` (R84) | Node ownership: HID only; sleep: koeurax only |
| Seed and overlays | shipped | profiles as data in `/lib/oath/seed` (R72); `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | `pkg:linux`, built by `pkg:gcc` (7.3; novus's drivers, KVM, nftables; TIOCSTI off, R60; Landlock, Yama, R64); vermagic checked; `pkg:linux-build` for outside modules (NVIDIA's, R34) | — |
| Boot | shipped | initrd: PID 1, bash, firmware; switch_root into `@`; `oath-efi` loader; the mark's boot steps, then `efidrm` (R89); 5 boots kept; `oath update` takes the newest signed output (R38), builds recipes, replaces overlays, follows the profile (R47, R62) | Kernels, PID 1's boot work need a reboot; a hung kernel shows `Starting Linux` |
| Image build | shipped | `cargo make build` packs `pkg:oath`, locked and pinned packs (R28); published only from a clean master (R47) | — |
| Install | shipped | installer ramdisk; GPT, ESP, `@` by UUID; `--overlay`; pins the host key; `--qemu`, `--usb`, `--local`; checked outputs; `oath install` (R43), `@home` apart (R45); `cargo make image` (R86) | Image untried on metal; Intel, newer AMD; AMI `UEFI OS` |
| Test harness | shipped | `cargo make check`: a QEMU per case, `check-pass` per output; `--target`: `metal` cases on a machine | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (rivulet, Sola) | partial | Sola's apps; PipeWire, realtime, BlueZ; Noto; libglvnd, RADV or NVIDIA, a pack (R89); H.264, AAC (R40); shortcut inhibiting (R44). Gaps: other apps; Intel GPUs; the KVM's capture, pairing (R44) |
| Wicket host | target | Left core (R1). |
| `writ`, coding agent | partial | R64–R70: boxed agent; local model (CPU/GPU) or OpenRouter (`writ`, `check`, `use`, `apply`); buccina, novus. Gaps: long OpenRouter use, `oath` tool |
| Steam + Minecraft desk | partial | Steam (R29), sandboxed: canto, novus, koeurax (games, a pad). Minecraft (R74): Prism, runs on koeurax |
