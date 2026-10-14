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
   product; the kernel is built by the host's gcc and the Oath crates by
   the host's rustc (with Rust's bundled musl); sources are
   sha256-pinned but upstream signatures are not checked; nothing
   enforces a licence allowlist on publish; build outputs have no
   manifest. Next: package cmake, ninja and python so LLVM can be
   rebuilt by a plan.

**State:** phases 1–5 are done ([docs/roadmap.md](docs/roadmap.md)).
The image is the native headless base: our PID 1 and `oath`, the
pinned kernel, and 15 packages built from `pkgs/` by our Clang/LLVM on
musl. 27 harness cases and the `install --qemu` rehearsal pass. Nothing
has been booted on metal since the cut.

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
