# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-15 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R11 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Reinstall buccina (phase 6).** canto is done. buccina answers at
   10.0.0.149 with a new host key and refuses the build host's key as
   root. Next: ask Joshua for the Wicket deploy key path, or write a
   `cargo make install --usb` stick for him to boot it from.
2. **Modalias loading (phase 6).** On canto PID 1 loads every module in
   the fixed list (virtio, nvme, e1000e, igb, r8169). Next: load by
   `modules.alias` from sysfs `modalias` files, keep display and sound
   deferred, ship every module the config builds, and prove it on canto
   with `cargo make check --target home@10.0.0.108`.
3. **Fixtures out of the product seed.** `base` ships `svc:hold`,
   `pkg:hello`, `pkg:fetchme` (a QEMU URL) and `svc:serial` to every
   machine. Next: have the QEMU harness add its fixtures, so the image
   it checks is still the one `install` ships.
4. **Native base leftovers (phase 5).** `pkg:llvm` is still a bootstrap
   product; the kernel is built by the host's gcc and the Oath crates by
   the host's rustc (with Rust's bundled musl); sources are
   sha256-pinned but upstream signatures are not checked; nothing
   enforces a licence allowlist on publish; build outputs have no
   manifest. Next: package cmake, ninja and python so LLVM can be
   rebuilt by a plan.

**State:** phases 1–5 are done; phase 6 is active
([docs/roadmap.md](docs/roadmap.md)). The image is the native headless
base. canto was wiped and reinstalled over SSH on 2026-10-14 and passes
all 22 `metal` harness cases (`cargo make check --target`). 27 QEMU
cases and the `install --qemu` rehearsal pass.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.108` (key in `build/known_hosts`) | Mac Pro 2013; native base since 2026-10-14; cable in the second port, so not 10.0.0.3; [hosts/canto.md](hosts/canto.md) |
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
