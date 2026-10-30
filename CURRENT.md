# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-30 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R43 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, Joshua 2026-10-28).** `cargo make publish` puts
   the newest checked output on the bucket, signed by buccina's key, and
   `sudo oath update --confirm --reboot` takes it (also in `oath mcp`).
   Published 2026-10-30: `fed6fb141a5e`, whose `oath` builds the
   owner's recipes (canto's Steam) first and restarts none of the
   seat's services; buccina runs it, canto `fdecb1ec3d68`. Gap: `svc`
   specs reach a machine only by hand. Next: canto's `sudo oath update
   --confirm --reboot`, when its desk is free; expat 2.10.0 (11-02).
2. **Steam on canto (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher, a recipe, in a bubblewrap sandbox; canto runs the
   64-bit client, signed in, with the i386 stack; PID 1 gives `home` the
   pads' nodes (R30). Next: a 32-bit game and a pad.
3. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22, [docs/specs/desk.md](docs/specs/desk.md)): rivulet, seatd,
   Xwayland, PipeWire, Noto, Vulkan and Sola's pinned packs (R28, R37);
   `pkg:cef` is CEF 156 with H.264 and AAC (R40). R42's login is in
   `fdecb1ec3d68`. R44 (2026-10-30): the KVM will capture through
   rivulet, which serves keyboard-shortcuts-inhibit (`pkg:rivulet`
   `be2a874d`), and pair by key; `svc:sola-kvm` is off in the profile.
   Next (Sola's session): `sola-kvm`'s capture and pairing; `sola-lock`,
   Settings → Login and Exit Sola tried on canto.
4. **novus on Oath (R32–R35, R41, R43, [hosts/novus.md](hosts/novus.md)).**
   `oath install` from the stick put desk output `cf238ebe3360` on the
   500 GB on 2026-10-30; novus boots it to the desk on the card. Its AMI
   firmware kept `UEFI OS` (its own entry for the 2 TB's old ESP), which
   the preview said it would remove, and did not start Oath first until
   Joshua set the order in setup. Next: home to the 2 TB, restored by
   hand from the 4 TB; then the installer on AMI firmware.
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
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; output `cf238ebe3360` (installed 2026-10-30 by `oath install`), `desk` with `pkg:nvidia`; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `fed6fb141a5e` (not rebooted since 2026-10-28), `desk`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

Oath work happens on **buccina** (R39). **novus**, Joshua's desk, runs Oath since 2026-10-30.

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
