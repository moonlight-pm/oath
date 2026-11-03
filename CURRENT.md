# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-11-02 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R59 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, R47).** `cargo make publish` (only from a clean
   master equal to origin/master) puts the newest checked output on the
   bucket, signed by buccina's key; `sudo oath update --confirm --reboot`
   takes it. An update replaces overlays unless `--keep`, makes objects new to
   the profile, and checks `/bin` and `/lib` names. Newest release: `debfd75332ea`
   (2026-11-03), on canto and buccina; novus runs `c9735fc14e50` with Sola overlays. Gap: a
   changed `svc` spec goes by hand. Next: an absent package new to the profile is made present
   (built, unreleased); `oath` checks PID 1.
2. **Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher, a recipe, in a bubblewrap sandbox; canto and novus
   (NVIDIA, since 2026-10-31) run the 64-bit client. No game has run yet.
   `pkg:nvidia` with DLSS's DLLs and its 32-bit GBM backend is in `bafa2d030a0a`.
   Next: Joshua moves novus; its smoke (native Vulkan, Floating Point, Proton, a pad).
3. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `d6343e8b` (on CEF 156.0.15):
   core `pkg:sola`, fourteen `pkg:sola-<app>`, `svc:sola-vault`:
   release `debfd75332ea` (68/68, 2026-11-03). Each machine reboots into it,
   then sets `keep_children` on `svc:sola-session` by hand. Canto: `sola-lock`, Settings →
   Login, Exit Sola. Next: Joshua asks for a pull on novus and decides each reboot.
4. **Servers from source (R49–R59).** Blocky, restic, rsync, PostgreSQL with
   PostGIS, our own `route` and `cell`, on `pkg:go`, with `pkg:certs` for roots; in no
   profile (R52), on the origin since `c9735fc14e50`. They run as `work`, which has no `sudo`. Bias against
   packages, intake gates (R57). PostgreSQL's real test is elsewhere, not on buccina.
   What could still go: [specs/dependencies.md](docs/specs/dependencies.md). `cell` ran
   as `work` under a `svc` on buccina, 2026-11-02. Next: a sandbox for project installs;
   image builds if asked.
5. **Security audit (R48).** `svc:audit` runs on buccina (`0 4 * * *`): scans, reads each pin
   against upstream's newest release, may publish. Left by Joshua's word: libsndfile, the
   toolchain rebuilds, flex, QEMU's 9p. An unattended run cannot build CEF (commands stop at
   2 h). `debfd75332ea` carries 12 io_uring patches on Linux 7.3.1 and libslirp 4.9.7 (14 CVEs,
   by Joshua's word, 2026-11-03); the patches come out at the first 7.3.y with the series.
   CVE-2026-99255 and 99262 wait for that 7.3.y. Next: the 2026-11-04 run publishes Codex
   0.165.0 and moves buccina (Joshua: "let the audit publish it"); a CEF rebuild for fribidi
   and expat 2.10.0, from 2026-11-10, is his call. R60 (TIOCSTI off in the kernel) is on master,
   its own release next.

**State:** phases 1–7 are done (6 on 2026-10-27: buccina passes every
metal case that applies, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). R36 (2026-10-26): account `void`;
OpenSSH's sshd replaces dropbear; dhcpcd and chronyd drop to `void`
(canto since 2026-10-28). Left until needed: root SSH, more NICs, node rules.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `3e42879d38e5` (booted 2026-11-01, `pkg:sola` overlay from master@ac660375), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `3e42879d38e5` (no overlay, not rebooted since that release), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `1c09695ec24e` (master `6d20da0`, unpublished, 2026-11-02), `desk`, `svc:audit`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |

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
