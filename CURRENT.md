# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-11-05 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R71 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Pull updates (R38, R47).** `cargo make publish` (only from a clean
   master equal to origin/master) puts the newest checked output on the
   bucket, signed by buccina's key; `sudo oath update --confirm --reboot`
   takes it. An update replaces overlays unless `--keep`, makes objects new to
   the profile, and checks `/bin` and `/lib` names. Newest release: `e555e1a5dd34`
   (2026-11-06: Sola `0c99a6a5` with `pkg:sola-oath`; `220c74bb6867` before it brought `host:local.seat_hid`, R71); buccina runs `f612b200206c`, novus `ab9ff2a9213f` (not rebooted, so `e42eaf977392`'s PID 1 until it boots), canto `debfd75332ea`. Gap: a
   changed `svc` spec goes by hand. Next: an absent package new to the profile is made present
   (built, unreleased); `oath` checks PID 1. R62 (in that release): a machine holds what its packages
   prescribe; `oath reap` each boot frees about 33 GB on novus, 59 GB on canto. R61–R63's verbs and fields
   are on novus, and reach canto with its next update; Sola's package app is its `sola-oath` session on novus.
2. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `0c99a6a5` (on CEF 156.0.15):
   core `pkg:sola`, fifteen `pkg:sola-<app>` (`pkg:sola-oath`, the read-only package app, new), `svc:sola-vault`;
   a pin moves PID 1's hash, so it takes a reboot. R71: `host:local.seat_hid` gives the desk a listed HID device's vendor interface (Sola's mouse battery); novus wants `["3554:f510"]` once it runs `220c74bb6867` or newer. Novus's Sola overlays (next@aa6e9f16) are the same code, so a plain update loses nothing. Sola's publish builds `plan:sola-build` with `--no-pkg`; the object is gone from novus and canto. Each machine reboots into it,
   then sets `keep_children` on `svc:sola-session` by hand. Canto: `sola-lock`, Settings →
   Login, Exit Sola. Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)): the 64-bit client runs, no game yet. Next: Steam's smoke on novus (Vulkan, Floating Point, Proton, a pad).
3. **Servers from source (R49–R59).** Blocky, `cloudflared`, restic, rsync, PostgreSQL with
   PostGIS, our own `route` and `cell`, on `pkg:go`, with `pkg:certs` for roots; in no
   profile (R52), on the origin since `c9735fc14e50` (`cloudflared` since 2026-11-03). They run as `work`, which has no `sudo`. Bias against
   packages, intake gates (R57). PostgreSQL's real test is elsewhere, not on buccina.
   What could still go: [specs/dependencies.md](docs/specs/dependencies.md). `cell` ran
   as `work` under a `svc` on buccina, 2026-11-02. Next: a sandbox for project installs;
   image builds if asked.
4. **Security audit (R48).** `svc:audit` runs on buccina (`0 4 * * *`): scans, reads each pin
   against upstream's newest release, may publish, and puts findings on the origin (R61). Left by Joshua's word: libsndfile, the
   toolchain rebuilds, flex, QEMU's 9p. Linux 7.3.1 carries 12 io_uring patches, out at the first 7.3.y with the series;
   CVE-2026-99255 and 99262 wait for that 7.3.y. rsync 3.5.2 (R67) is on the origin; its fix bounds only unbroken collision runs.
   R67: the audit pins a key upstream names and rebuilds CEF itself, the runner carrying what outlasts a 2 h command; neither has run for real.
   Next: the 2026-11-10 run moves fribidi 1.0.18, expat 2.10.0, vulkan-loader and vulkan-tools 1.4.367 and rebuilds CEF, when all are a week old
   (Joshua, 2026-11-04); buccina takes it at the next run. R60 is on buccina; Joshua left canto on `debfd75332ea` for now.

**State:** phases 1–7 are done (6 on 2026-10-27, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). Left until needed: root SSH, more NICs, node rules.

5. **`writ`, Oath's coding agent (R64–R70, [docs/specs/writ.md](docs/specs/writ.md)).** A model works on a project in a box: `writ` is a session on a terminal, which loads the local model as `work`
   (`/llama stop` frees it) or reaches one on OpenRouter (`writ key`, `/use openrouter AUTHOR/NAME`, R70); its writes wait for `/diff`, `/apply`, `/undo`. Overlays on buccina (`b4f6043`) and novus (`5c4bce3`, from the origin; a 40% GPU share, no key yet). Joshua has used it on Kirk, and after a fix to its prompt it did well; Qwen3.5 9B has also reported work falsely, so read its diffs, not its summaries. OpenRouter works: on 2026-11-05 DeepSeek V4.1 Flash did real work for fractions of a cent, 86–98% cached, and six other families did a planted bug. Next: long use, since no real rate limit or failed stream has been met. Qwen3.8 27B now fits beside the desk: on 2026-11-05 Bonsai `Q2_0` (9.2 GiB on the card) and ISTA `IQ3_XXS` (11.5, the cleanest) each did a real fix twice at 17–20 tokens/s, which Qwen3.5 9B did not finish either time; ISTA's larger `IQ3_S` and unsloth's `UD-Q4_K_M` did no better. Drafting with the model's MTP layer made each file about 1.5× faster, and `writ use local --mtp` keeps it on: since 2026-11-05 novus's local model is `IQ3_XXS-mtp` with it (Joshua's choice: 23.8 tokens/s, the real fix in 5.1 minutes, 12.5 GiB), and a session's first lines say `MTP drafting on`. The first prompt after a load still waits about 100 s. The other trial files are deleted.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `debfd75332ea` (2026-11-03, no overlay), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `f612b200206c` (generation 85, 2026-11-05, not rebooted since 2026-11-04; fifteen `pkg:sola*` overlays from Sola's next), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `e73aa522ec58` (master `1678f60`, 2026-11-04), headless: `desk`'s packages with its services off, `svc:audit`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |
| dulcia | `ssh dulcia` from buccina (`home@10.0.0.169`, key in `build/known_hosts`) | OptiPlex 5080, i7-10700T, 64 GB; a server on `base`, installed 2026-11-06 from the stick: output `01f12aaf944d` (generation 1); no `base` release on the origin yet, so `oath update` has nothing to take; [hosts/dulcia.md](hosts/dulcia.md) |

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
