# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-29 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R42 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, Joshua 2026-10-28).** `cargo make publish` puts
   the newest checked output on the bucket, signed by buccina's key, and
   `sudo oath update --confirm --reboot` takes it (also in `oath mcp`).
   Published: `fdecb1ec3d68` (R41, R42), on buccina and canto. Built:
   `fed6fb141a5e`, whose `oath` builds the owner's recipes (canto's
   Steam) first and restarts none of the seat's services. Gap: `svc`
   specs reach a machine only by hand. Next: publish it on Joshua's
   word; expat 2.10.0 (11-02).
2. **Steam on canto (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher, a recipe, in a bubblewrap sandbox; canto runs the
   64-bit client, signed in, with the i386 stack; PID 1 gives `home` the
   pads' nodes (R30). Next: a 32-bit game and a pad.
3. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22, [docs/specs/desk.md](docs/specs/desk.md)): rivulet, seatd,
   Xwayland, PipeWire, Noto, Vulkan and Sola's pinned packs (R28).
   `pkg:cef` is CEF 156 with H.264 and AAC (R40, `2a2cc750`); Sola's
   packs from `83df4558` are built on it and pinned (R37 done
   2026-10-29). R42: `svc:login`, the seat's turns, `oath password`,
   `oath desk` ([docs/specs/login.md](docs/specs/login.md)), published
   in `fdecb1ec3d68`; canto has its objects (generation 411). Next:
   Sola's `sola-lock`, Settings → Login and Exit Sola tried on canto.
4. **novus to Oath (R32–R35, [hosts/novus.md](hosts/novus.md)).** NixOS
   goes; novus gets its own Oath install. Built: bootstrap on an Oath
   host, `pkg:qemu`, KVM, novus's drivers, NVIDIA 595.104.02 as a recipe
   in `desk`, a root by btrfs UUID, `install --local`. buccina did a
   build host's whole job (2026-10-27). R41 (2026-10-29): libglvnd and
   NVIDIA's lookups for the desk, BlueZ on a system bus, the fan curve;
   the trial stick is desk output `67d5d5c1160c` with `pkg:nvidia` built
   and on. Next: Joshua boots novus from it (F8); from buccina, the
   desk on the card, sound, pairing, fans, `nvidia-smi`.
5. **Recipes and origins (R23, R25, R31).** Every catalog carries the
   coding agents' plans (`claude-code` 2.1.292, `codex` 0.164.0,
   `grok-build` 1.0.70) and `plan:steam`; machines fetch packs from
   `host:local.origins`, the R2 bucket written only by `cargo make
   publish` (buccina's, R38). Next: a new stable is a new pin; publish
   after every build that changes a pack.

**State:** phases 1–7 are done (6 on 2026-10-27: buccina passes every
metal case that applies, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). R36 (2026-10-26): account `void`;
OpenSSH's sshd replaces dropbear; dhcpcd and chronyd drop to `void`
(canto since 2026-10-28). Left until needed: root SSH, more NICs, node rules.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `fdecb1ec3d68` (booted 2026-10-29), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `fdecb1ec3d68` (not rebooted), `desk`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

Oath work happens on **buccina** (R39). **novus** (NixOS) is Joshua's desk, to become Oath ([hosts/novus.md](hosts/novus.md)).

## Do not

- `cargo make install --confirm` (wipes a disk) on anything but canto
  and buccina unless Joshua asks; install, update, reboot and wipe
  those two without asking (Joshua, 2026-10-18 and 2026-10-26).
- `sudo oath update --confirm` (writes a machine's ESP and store) on
  anything else unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS, or write its disks, before Joshua says so (R32).
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
