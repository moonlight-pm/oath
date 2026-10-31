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
   the profile made, a `/bin`/`/lib` name check. Release B (`dd0f847aa9c9`,
   the Sola split) since 2026-10-31 on canto, novus and buccina. Gap: a changed `svc` spec goes
   by hand. Next: an absent package new to the profile is made present
   (built, unreleased); `oath` checks PID 1; expat 2.10.0.
2. **Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher, a recipe, in a bubblewrap sandbox; canto and novus
   (NVIDIA, since 2026-10-31) run the 64-bit client. No game has run yet.
   `pkg:nvidia` with DLSS's DLLs and its 32-bit GBM backend is in `bafa2d030a0a`.
   Next: Joshua moves novus; its smoke (native Vulkan, Floating Point, Proton, a pad).
3. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `92815a65` split: core
   `pkg:sola` (`share/`) and twelve `pkg:sola-<app>`, with `source`;
   `svc:sola-sessions` on (`--restore`): Release B, on canto (booted) and
   novus (reboot next), each with `svc:sola-sessions` made by hand.
   Canto: `sola-lock`, Settings → Login, Exit Sola.
4. **novus on Oath (R32–R35, R41, R43, R45, [hosts/novus.md](hosts/novus.md)).**
   `oath install` put Oath on the 500 GB (2026-10-30); it runs
   `c40249fd9724` (since generation 7, 2026-10-31) with `/home` on the 2 TB
   (`@oath-home`, `host:local.mounts`, R45), the desk on the card and
   21 services. Its AMI firmware keeps `UEFI OS` and needed Oath set first
   by hand. Next: Joshua restores what he picks from the 4 TB into /home.
5. **Security audit (R48).** `cargo make audit` matches every package
   (`pkgs/audit.toml`) against CVE List V5, OSV, NVD, KEV and EPSS: 20 with
   hits on 2026-10-31 (expat 2.9.0, libsndfile, the kernel 7.3, …).
   `svc.schedule` (PID 1, croner) and `check/audit/` (runner, skill
   `oath-audit`, permissions, Resend mail to Joshua) are built; output
   `4456dda47a6d` (`4907549`, 65/65 with `svc-schedule`), unpublished.
   Next: Joshua starts the first run by hand; buccina on a release with
   `schedule`, then `oath new svc:audit` (`0 4 * * *`).

**State:** phases 1–7 are done (6 on 2026-10-27: buccina passes every
metal case that applies, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). R36 (2026-10-26): account `void`;
OpenSSH's sshd replaces dropbear; dhcpcd and chronyd drop to `void`
(canto since 2026-10-28). Left until needed: root SSH, more NICs, node rules.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `dd0f847aa9c9` (Release B, booted 2026-10-31), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `dd0f847aa9c9` (Release B, generation 26; reboot next), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `dd0f847aa9c9` (Release B, booted 2026-10-31), `desk`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

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
