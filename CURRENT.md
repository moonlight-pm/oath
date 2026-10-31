# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-31 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R46 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, R47).** `cargo make publish` (only from a clean
   master equal to origin/master) puts the newest checked output on the
   bucket, signed by buccina's key; `sudo oath update --confirm --reboot`
   takes it. Published 2026-10-31: Release A, `940c5d970175` (`ddcebae`,
   64/64): `pkg.source`, overlays replaced unless `--keep`, objects new to
   the profile made, a `/bin`/`/lib` name check. novus and canto run
   `c40249fd9724`, buccina `fed6fb141a5e`. Gap: a changed `svc` spec goes
   by hand. Next: novus and canto take A (Sola's session; plain
   `--confirm`, their old `oath` keeps Sola's pins); buccina with
   `--reboot`; `oath` checks PID 1; expat 2.10.0 (11-02).
2. **Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher, a recipe, in a bubblewrap sandbox; canto and novus
   (NVIDIA, since 2026-10-31) run the 64-bit client. No game has run yet.
   `pkg:nvidia` with DLSS's DLLs and its 32-bit GBM backend is in `bafa2d030a0a`.
   Next: Joshua moves novus; its smoke (native Vulkan, Floating Point, Proton, a pad).
3. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` (Sola `25f2be60`) drives canto since
   2026-10-31. Off in the profile: `svc:sola-kvm`, and `svc:sola-sessions`
   until `pkg:sola` pins `--restore`. R47: Sola splits `pkg:sola` (core,
   `share/`) from `pkg:sola-<app>`. Next: after Release A, one commit pins
   the split with `source`, turns `svc:sola-sessions` on (exec plain once
   `--restore` waits itself): Release B. Canto: `sola-lock`, Settings →
   Login, Exit Sola.
4. **novus on Oath (R32–R35, R41, R43, R45, [hosts/novus.md](hosts/novus.md)).**
   `oath install` put Oath on the 500 GB (2026-10-30); it runs
   `c40249fd9724` (since generation 7, 2026-10-31) with `/home` on the 2 TB
   (`@oath-home`, `host:local.mounts`, R45), the desk on the card and
   21 services. Its AMI firmware keeps `UEFI OS` and needed Oath set first
   by hand. Next: Joshua restores what he picks from the 4 TB into /home.
5. **Recipes, origins and npm (R23, R25, R31, R46).** Every catalog
   carries the coding agents' plans (`claude-code` 2.1.292, `codex`
   0.164.0, `grok-build` 1.0.70) and `plan:steam`; `pkg:node` 26.13.0,
   `pkg:gh` 2.103.3 and `pkg:cf` (npm, in its own tree) are on the origin
   in no profile; novus has those and claude and grok (2026-10-31).
   Next: Joshua's security audit (pins, advisories, the npm and GitHub
   signatures oath-make does not check); publish after every pack build.

**State:** phases 1–7 are done (6 on 2026-10-27: buccina passes every
metal case that applies, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). R36 (2026-10-26): account `void`;
OpenSSH's sshd replaces dropbear; dhcpcd and chronyd drop to `void`
(canto since 2026-10-28). Left until needed: root SSH, more NICs, node rules.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `c40249fd9724` (booted 2026-10-31), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `c40249fd9724` (generation 13: Steam), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `fed6fb141a5e` (not rebooted since 2026-10-28), `desk`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

## Do not

- `cargo make install --confirm` (wipes a disk) on anything but canto
  and buccina unless Joshua asks; install, update, reboot and wipe
  those two without asking (Joshua, 2026-10-18 and 2026-10-26).
- `sudo oath update --confirm` (writes a machine's ESP and store) on
  anything else unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus, update it or write its disks without Joshua's word (R32).
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
