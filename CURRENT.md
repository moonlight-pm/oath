# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-16 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R13 in [docs/decisions.md](docs/decisions.md)

## Now

1. **canto onto the new base (needs Joshua).** canto runs output
   `97c1f7409deb` (pruned from `build/out`); every target short of the
   desk landed after it. Next, when Joshua says so:
   `cargo make update --target home@10.0.0.3 --confirm --reboot`, then
   `cargo make check --target home@10.0.0.3` (31 metal cases now).
2. **Spec/status split (Open).** The last catalog target: whether status
   leaves the snapshotted tree (docs/decisions.md). Next: Joshua decides.
3. **Phase 7, agent surface.** MCP and agent tooling over `oath --json`.
   Next: a spec in `docs/specs/` for the adapter, then build it.
4. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, a
   `cargo make install --usb` stick, or `cargo make update` once it
   runs an Oath base.

**State:** phases 1–5 are done; phase 6 is active
([docs/roadmap.md](docs/roadmap.md)). On 2026-10-16 every target in the
specs short of phase 8 was built, except the spec/status split: source
beside each pack on origins (R13), probation with automatic rollback,
store integrity (`oath verify`), schemas generated from the types,
cycle reasons and reverse-`wants` shutdown, DHCP renewals in actual,
device discovery and hotplug, PID 1 surviving its own panic, power
changes without a generation, install's build-host guard and
`cargo make update`. The newest output passes 40 QEMU cases, and
`cargo make update --qemu` moved an installed older output to a newer
one. Left by design until something needs them: root SSH as a field,
more NICs, device rules as fields, RPATHs to hash paths (the desk).

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013; native base, LLVM kernel since 2026-10-16; [hosts/canto.md](hosts/canto.md) |
| buccina | 10.0.0.149, no working key | microserver; Wicket trial image, not reinstalled; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS). Not an Oath machine.

## Do not

- `cargo make install --confirm` (wipes a disk) unless Joshua asks.
- `cargo make update --target` (writes a real machine's ESP and store)
  unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS as if it were Oath.
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
