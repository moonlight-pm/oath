# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-13 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R8 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Cut (review phase 1) — done 2026-10-13.** Desk, T37, T39, Wicket,
   probe and the doc diary are out (tag `t37-t39-final`). Headless
   `base` image (38 MB qcow) boots in QEMU on the own-built kernel;
   SSH as `home`, set/apply/undo, `pkg:hello`, `svc:hold` and reboot
   persistence checked by hand; a fresh boot is in-sync. Next: nothing;
   the harness below turns these hand checks into a gate.
2. **Harness (phase 2, R5).** `cargo make check`: boot headless QEMU,
   run the README loop and the known failure cases over SSH from a
   data file, exit non-zero on any miss. Cases that fail today
   (torn write, wants cycle, failed apply then undo, sshd lockout)
   become the acceptance tests for 3 and 4. It must also serve
   `pkg:fetchme` at 10.0.2.2:18765 (the probe used to). Next: write
   `check/cases.toml` and the runner in `crates/oath-make`.
3. **Catalog safety (phase 3).** Locked temp-file-then-rename
   writes; validate before snapshot; log every outcome; undo restores
   spec only; `oath new` / `oath rm`; typed `set`; id grammar;
   probation apply for lockout-class changes. R4 (flock + rename) is
   decided. Next: after the harness.
4. **PID 1 (phase 4).** Log ring + `oath logs`, exit codes, backoff,
   failed state, cgroups, shutdown, `switch_root`, panic hook, a
   cycle skips one service, DHCP as a service, modalias loading.
   Next: after 3.
5. **Native base (phase 5, R7).** Every package is built from source
   by an Oath plan or is the vendor's release pinned by sha256.
   Today `image/tools.nix` still supplies busybox, btrfs-progs,
   dropbear, openssh client bits, curl, sgdisk, mkfs.fat, kexec, tar,
   systemd-boot, firmware and OVMF. Next: Joshua confirms Clang/LLVM
   (proposed) over GCC; then stage-0 LLVM on novus and busybox as the
   first plan. No Zig (R9).

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
