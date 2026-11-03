# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-11-03 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R65 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, R47).** `cargo make publish` (only from a clean
   master equal to origin/master) puts the newest checked output on the
   bucket, signed by buccina's key; `sudo oath update --confirm --reboot`
   takes it. An update replaces overlays unless `--keep`, makes objects new to
   the profile, and checks `/bin` and `/lib` names. Newest release: `ca0f80bd441a`
   (2026-11-03), on buccina; canto runs `debfd75332ea`, novus `c9735fc14e50`. Gap: a
   changed `svc` spec goes by hand. Next: an absent package new to the profile is made present
   (built, unreleased); `oath` checks PID 1. R62 (no release yet): a machine holds what its packages
   prescribe; `oath reap` each boot frees about 33 GB on novus, 59 GB on canto. On canto and novus,
   `pkg:lzo` (and canto's `pkg:dropbear`) predate it and go by hand, Joshua's call.
2. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `d6343e8b` (on CEF 156.0.15):
   core `pkg:sola`, fourteen `pkg:sola-<app>`, `svc:sola-vault`:
   release `ca0f80bd441a` (69/69, 2026-11-03). Each machine reboots into it,
   then sets `keep_children` on `svc:sola-session` by hand. Canto: `sola-lock`, Settings →
   Login, Exit Sola. Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)): the 64-bit client runs, no game yet. Next: Joshua asks
   for a pull on novus and decides each reboot, then Steam's smoke there (Vulkan, Floating Point, Proton, a pad).
3. **Servers from source (R49–R59).** Blocky, restic, rsync, PostgreSQL with
   PostGIS, our own `route` and `cell`, on `pkg:go`, with `pkg:certs` for roots; in no
   profile (R52), on the origin since `c9735fc14e50`. They run as `work`, which has no `sudo`. Bias against
   packages, intake gates (R57). PostgreSQL's real test is elsewhere, not on buccina.
   What could still go: [specs/dependencies.md](docs/specs/dependencies.md). `cell` ran
   as `work` under a `svc` on buccina, 2026-11-02. Next: a sandbox for project installs;
   image builds if asked.
4. **Security audit (R48).** `svc:audit` runs on buccina (`0 4 * * *`): scans, reads each pin
   against upstream's newest release, may publish, and puts findings on the origin (R61). Left by Joshua's word: libsndfile, the
   toolchain rebuilds, flex, QEMU's 9p. An unattended run cannot build CEF (commands stop at
   2 h). `debfd75332ea` carries 12 io_uring patches on Linux 7.3.1 and libslirp 4.9.7 (14 CVEs,
   by Joshua's word, 2026-11-03); the patches come out at the first 7.3.y with the series.
   CVE-2026-99255 and 99262 wait for that 7.3.y. Next: the 2026-11-04 run publishes Codex
   0.165.0 and moves buccina (Joshua: "let the audit publish it"); a CEF rebuild for fribidi
   and expat 2.10.0, from 2026-11-10, is his call. R60 (TIOCSTI off in the kernel) is in `ca0f80bd441a`, on
   buccina; a machine has it from the reboot into it, and Joshua left canto on `debfd75332ea` for now.

**State:** phases 1–7 are done (6 on 2026-10-27, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). Left until needed: root SSH, more NICs, node rules.

5. **`writ`, Oath's coding agent (R64, R65, [docs/specs/writ.md](docs/specs/writ.md)).** Its box is built, and a local
   model runs in one: `pkg:llama-cpp` under `writ serve`, on a socket only `home` reaches, overlays on buccina. `writ ask`
   reads its stream; a stream cut anywhere runs nothing. Landlock and Yama are in no release yet (Joshua's word). Next:
   tools and the loop on a local model, Vulkan for novus's GPU, then OpenRouter.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `debfd75332ea` (2026-11-03, no overlay), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `c9735fc14e50` (booted 2026-11-02, 15 `pkg:sola*` overlays from master@822bb2dd), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `ca0f80bd441a` (master `75def41`, 2026-11-03), headless: `desk`'s packages with its services off, `svc:audit`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

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
