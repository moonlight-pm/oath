# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-16 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R12 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Native base leftovers (phase 5).** The Oath crates are built by
   the host's rustc (with Rust's bundled musl); sources are
   sha256-pinned but upstream signatures are not checked; nothing
   enforces a licence allowlist on publish; build outputs have no
   manifest. Next: the vendor's Rust release pinned by sha256 as a
   pack, so `cargo make build` stops using the host's rustc.
2. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, or a
   `cargo make install --usb` stick.

**State:** phases 1–5 are done; phase 6 is active
([docs/roadmap.md](docs/roadmap.md)). The image is the native headless
base, its kernel and every package are plan products of `pkgs/`, and
its seed carries no test fixtures: the harness checks the
image `install` ships, and each case makes what it needs with
`oath new`. Modules load by modalias; `svc:time` keeps the clock from
pool.ntp.org. canto was reinstalled over SSH, unattended, from that image on
2026-10-16 (the kernel built by `pkg:llvm`) and passes all 24 `metal`
harness cases (`cargo make check --target`).
30 QEMU cases and the `install --qemu` rehearsal pass.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013; native base, LLVM kernel since 2026-10-16; [hosts/canto.md](hosts/canto.md) |
| buccina | 10.0.0.149, no working key | microserver; Wicket trial image, not reinstalled; [hosts/buccina.md](hosts/buccina.md) |

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
