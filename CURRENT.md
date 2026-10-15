# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-15 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R11 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Prove modalias loading on canto (phase 6).** Built and passing in
   QEMU: PID 1 loads drivers by `modules.alias` and is the kernel's
   modprobe helper. canto is off the network since its cable moved to
   the first port (the NIC is chosen at boot). Next: Joshua
   power-cycles it; pin its key for its new address, reinstall it, and
   run `cargo make check --target`; confirm `/proc/modules` holds tg3
   and no virtio.
2. **Fixtures out of the product seed.** `base` ships `svc:hold`,
   `pkg:hello`, `pkg:fetchme` (a QEMU URL) and `svc:serial` to every
   machine. Next: have the QEMU harness add its fixtures, so the image
   it checks is still the one `install` ships.
3. **Native base leftovers (phase 5).** `pkg:llvm` is still a bootstrap
   product; the kernel is built by the host's gcc and the Oath crates by
   the host's rustc (with Rust's bundled musl); sources are
   sha256-pinned but upstream signatures are not checked; nothing
   enforces a licence allowlist on publish; build outputs have no
   manifest. Next: package cmake, ninja and python so LLVM can be
   rebuilt by a plan.
4. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, or a
   `cargo make install --usb` stick.

**State:** phases 1–5 are done; phase 6 is active
([docs/roadmap.md](docs/roadmap.md)). The image is the native headless
base. canto was wiped and reinstalled over SSH on 2026-10-14 and passes
all 22 `metal` harness cases (`cargo make check --target`). Modules
load by modalias since 2026-10-15 (QEMU only so far). 29 QEMU cases and
the `install --qemu` rehearsal pass.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | off the network until power-cycled (cable moved) | Mac Pro 2013; native base since 2026-10-14; key pinned for 10.0.0.108 in `build/known_hosts`; [hosts/canto.md](hosts/canto.md) |
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
