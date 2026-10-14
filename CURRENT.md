# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-14 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R11 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Reinstall canto and buccina (phase 6)** from a clean native build,
   run the harness cases against them over SSH, and add modalias module
   loading with metal in hand. Only when Joshua asks. Next: ask Joshua
   whether to start, or whether phase 7 (agent surface) or phase 8
   (native desk) comes first.
2. **Native base leftovers (phase 5).** `pkg:llvm` is still a bootstrap
   product; the kernel is still built by the host's gcc; sources are
   sha256-pinned but upstream signatures are not checked; `lock.json`
   carries licences but nothing enforces a publish allowlist yet; build
   outputs have no manifest. Next: package cmake, ninja and python so
   LLVM can be rebuilt by a plan.

Done 2026-10-13: the cut (tag `t37-t39-final`), Zig removed, the
harness (`cargo make check`, one QEMU per case, `install` requires its
stamp), catalog safety (phase 3: locked atomic writes, typed `set`, id
grammar, checks before the snapshot, every outcome logged, a stack undo
of spec only, `oath new` / `oath rm`, `--confirm` for changes that can
cut off SSH, PID 1 running only applied specs), and PID 1 (phase 4:
`oath logs`, exit codes, backoff into `failed`, a cgroup per service,
orderly shutdown, switch_root, `panic=10`, a supervised DHCP client,
apply waiting for PID 1).

Done 2026-10-14: the native base (phase 5, R11). `cargo make bootstrap`
builds Clang/LLVM 23.1.3 on musl 1.2.6 (stage 0 by host gcc, never
ships; stage 1 `pkg:llvm`); `cargo make pkgs` builds 15 packages
from `pkgs/` as pinned plans, bit-identical on rebuild. The image takes
all userspace from `pkgs/lock.json`; `image/tools.nix` and systemd-boot
are gone; the installer uses sfdisk, wipefs and native mkfs. PID 1 again
restores 0666 on `/dev/null` and friends after `mdev -s`. 27 cases and
the `install --qemu` rehearsal pass. Carried forward: generated
schemas, probation with auto-revert, store integrity at boot, modalias
loading, reverse-`wants` shutdown order.

## Machines

Both still run their pre-cut images and are not managed from this
tree until they are wiped and reinstalled (phase 6).

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
