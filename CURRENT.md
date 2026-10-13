# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-13 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R8 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Native base (phase 5, R7) — next, blocked.** Needs Joshua's yes on
   Clang/LLVM as the system compiler (decisions.md; Zig is out). Once
   confirmed: build stage-0 LLVM (clang, lld, compiler-rt, libunwind,
   libc++) on novus against musl, then busybox as the first `plan`,
   then btrfs-progs, dropbear, curl, sgdisk, dosfstools and firmware;
   retire `image/tools.nix` piece by piece, keeping `cargo make check`
   green. Next: ask Joshua to confirm the compiler.
2. **Reinstall canto and buccina (phase 6)** from the native base, then
   run the harness cases against them over SSH, and add modalias module
   loading with metal in hand. Only when Joshua asks.

Done 2026-10-13: the cut (tag `t37-t39-final`), Zig removed, the
harness (`cargo make check`, one QEMU per case, `install` requires its
stamp), catalog safety (phase 3: locked atomic writes, typed `set`, id
grammar, checks before the snapshot, every outcome logged, a stack undo
of spec only, `oath new` / `oath rm`, `--confirm` for changes that can
cut off SSH, PID 1 running only applied specs), and PID 1 (phase 4:
`oath logs`, exit codes, backoff into `failed`, a cgroup per service,
orderly shutdown on `reboot` / `poweroff`, switch_root, `panic=10`, a
supervised DHCP client, apply waiting for PID 1). 26 cases, all pass.
Carried forward: generated schemas, probation with auto-revert, store
integrity at boot, modalias loading, reverse-`wants` shutdown order.
Correction: an early hand "reboot persistence" check never rebooted
(QEMU runs `-no-reboot`); the harness checks real reboots and they pass.

## Machines

Both still run their pre-cut images and are not managed from this
tree until they are wiped and reinstalled (after 5).

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@canto` (10.0.0.3) | Mac Pro 2013; desk image from 2026-09; [hosts/canto.md](hosts/canto.md) |
| buccina | `ssh root@10.0.0.149` (Wicket deploy key) | microserver; Wicket trial image; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS). Not an Oath machine.

## Do not

- `cargo make install --confirm` (wipes a disk) unless Joshua asks.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS as if it were Oath.
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
