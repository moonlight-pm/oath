# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-13 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R8 in [docs/decisions.md](docs/decisions.md)

## Now

1. **PID 1 (phase 4) — active.** Gate: the `reboot-command` gap case
   in `check/cases.toml` turns PASS, plus new cases for each item. In
   order: a log ring per service and `oath logs svc:<name>`; exit code,
   a real restart count, backoff and a `failed` state in actual; a
   shutdown sequence (plain `reboot` and `power=reboot` stop services
   in reverse `wants` order, sync, unmount); one cgroup per service;
   `switch_root` instead of `chroot`; a panic hook and `panic=10` on
   metal; DHCP as a supervised service; modules by modalias. Next: the
   log ring and `oath logs`.
2. **Native base (phase 5, R7).** Needs Joshua's yes on Clang/LLVM as
   the system compiler (decisions.md). Then stage-0 LLVM on novus and
   busybox as the first plan; retire `image/tools.nix` piece by piece.
3. **Reinstall canto and buccina (phase 6)** from the native base,
   then run the harness cases against them over SSH. Only when Joshua
   asks.

Done 2026-10-13: the cut (tag `t37-t39-final`), Zig removed, the
harness (`cargo make check`, one QEMU per case, `install` requires its
stamp), and catalog safety (phase 3): locked atomic writes, typed
`set`, id grammar, checks before the snapshot, every outcome logged,
a stack undo of spec only, `oath new` / `oath rm`, `--confirm` for
changes that can cut off SSH, PID 1 running only applied specs, cycle
isolation at boot. 19 cases: 18 pass, 1 known gap (plain `reboot`).
Carried forward from phase 3: generated schemas, probation with
auto-revert, store integrity at boot. Correction: an early hand
"reboot persistence" check never rebooted (QEMU runs `-no-reboot`);
the harness checks a real catalog reboot and it passes.

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
- Write a USB stick or real disk he did not ask for.
- Change novus's OS as if it were Oath.
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
