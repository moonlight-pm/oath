# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-26 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R37 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Security sweep (Joshua, 2026-10-27).** Every package checked
   upstream: Linux 7.3 (R10), glibc's 2.44 branch, libstdc++ fixes, CA
   bundle and NSS 3.132 (no eMudhra), Python 3.15.2 (ssl), BIND 9.20.30
   and kernel.org's signed tarball (R37), patch, elfutils, vim, harfbuzz;
   unreleased fixes as `patches/`. novus and buccina built all 162
   products identically; buccina runs it (`f443df24beec`, published).
   Next: canto and novus when Joshua says; CEF 156 (R37); expat 2.10.0
   (11-02).
2. **Steam on canto (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher 1.0.0.87, a recipe, in a bubblewrap sandbox; canto
   runs the 64-bit client, signed in, with the i386 stack (`glibc-i386`,
   `mesa-i386` and 15 more). PID 1 gives `home` the pads' nodes (R30)
   since the boot of 2026-10-25. Next: a 32-bit game and a pad.
3. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22, [docs/specs/desk.md](docs/specs/desk.md)): rivulet, seatd,
   Xwayland, PipeWire, Noto, Vulkan (RADV), `wl-clipboard` (pasting
   screenshots, 2026-10-26) and Sola's packs, which `desk.json` pins
   (R28, no fork; `next` at `c6cda58b`). Next: realtime priority for
   PipeWire; new pins when the Sola tab publishes.
4. **novus to Oath (R32–R35, [hosts/novus.md](hosts/novus.md)).** NixOS
   goes; novus gets its own Oath install. Built: bootstrap on an Oath
   host, `pkg:qemu`, KVM, novus's drivers, NVIDIA 595.104.02 as a recipe
   in `desk`, a root by btrfs UUID, `install --local`, `pkg:bison` and
   `pkg:m4`. Self-host on buccina (2026-10-27, `b34d189`: 2h24m, the
   base settled in 4 rounds) built all 163 products as committed, bit
   for bit. Next: Joshua boots novus from the trial stick (pre-R36,
   dropbear; fine for the trial); from canto, NICs, audio, Bluetooth,
   sensors, the display, `plan:nvidia`, `svc:gpu-power`; then libglvnd
   and glvnd Mesa.
5. **Recipes and origins (R23, R25, R31).** Every catalog carries the
   coding agents' plans (`claude-code` 2.1.292, `codex` 0.164.0,
   `grok-build` 1.0.70) and `plan:steam`; machines fetch packs from
   `host:local.origins`, the R2 bucket written only by `cargo make
   publish`. Next: a new stable is a new pin; publish after every build
   that changes a pack; sign `INDEX.md` when a second publisher appears.

**State:** phases 1–7 are done (6 on 2026-10-27: buccina passes every
metal case that applies, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). R36 (2026-10-26): account `void`;
OpenSSH's sshd replaces dropbear; dhcpcd and chronyd drop to `void`;
canto (`55c20bf6a86c`, generation 329) keeps dropbear until its next
`update --reboot`. Left until needed: root SSH, more NICs, node rules.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `55c20bf6a86c`, the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` works from it; [hosts/canto.md](hosts/canto.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; the Oath build machine (self-host); output `f443df24beec`, `desk`; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS), to become Oath ([hosts/novus.md](hosts/novus.md)).

## Do not

- `cargo make install --confirm` (wipes a disk) on anything but canto
  and buccina unless Joshua asks; install, update, reboot and wipe
  those two without asking (Joshua, 2026-10-18 and 2026-10-26).
- `cargo make update --target` (writes a real machine's ESP and store)
  on anything else unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS, or write its disks, before Joshua says so (R32).
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
