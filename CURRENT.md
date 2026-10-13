# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-13 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R8 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Catalog safety (phase 3) — active.** Gate: the phase 3 gap cases
   in `check/cases.toml` turn PASS. In order: catalog file lock (R4,
   decided) and temp-file-then-rename writes; typed validation of every
   selected object before the snapshot; log every outcome, failures
   included; undo restores spec only (not `log/` or `store/`);
   `oath new` / `oath rm`; typed `set`; id grammar; probation apply
   for lockout-class changes (sshd, net). Next: the lock and atomic
   writes in `oath-core`.
2. **PID 1 (phase 4).** Reads `applied` only; log ring + `oath logs`,
   exit codes, backoff, failed state, cgroups, shutdown on `reboot`,
   `switch_root`, panic hook, a cycle isolates one service, DHCP as a
   service, modalias loading. Gate: the phase 4 gap cases.
3. **Native base (phase 5, R7).** Needs Joshua's yes on Clang/LLVM as
   the system compiler (decisions.md). Then stage-0 LLVM on novus and
   busybox as the first plan; retire `image/tools.nix` piece by piece.
4. **Reinstall canto and buccina (phase 6)** from the native base,
   then run the harness cases against them over SSH. Only when Joshua
   asks.

Done 2026-10-13: the cut (tag `t37-t39-final`), Zig removed, and the
harness: `cargo make check` boots one QEMU per case, 10 pass and 8
known gaps (phase 3–4), and `install` requires its stamp. Correction:
an earlier hand "reboot persistence" check never rebooted (QEMU runs
`-no-reboot`, plain `reboot` does nothing); the harness now checks a
real catalog reboot and it passes.

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
