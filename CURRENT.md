# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-11-08 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R75 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Packages on their own (R72), PID 1 live (R73).** `oath publish pkg:<name>` puts this
   machine's pin up as its head (`pkg/<name>.json`, signed `oath-pkg`), and `oath update` moves
   each package to the newest head a key in `host:local.publishers` may sign for its name
   (`names="sola,sola-*"` scopes a key: heads only, no release); a need moves only when the package
   keeps every library it loads, else the dependency waits and says why. Seed profiles are data in
   `/lib/oath/seed`, so a pin no longer moves PID 1, and `oath update` re-execs PID 1 into the
   output's binary with every service running; a kernel, PID 1's boot work and the first update from
   a pre-R73 PID 1 still reboot. Released 2026-11-07 from `7597cc1`: `desk` `6c8d38bd54c4` (Sola
   `0c99a6a5`; the audit's `bda181db1fc6` had R72 and R73 too), on novus and buccina, and `base`
   `ee3e239caa32`, on dulcia; canto runs its unpublished successor `06e0194eb4f3`. novus, canto and dulcia trust key `oath@novus` for `names="sola,sola-*"`;
   buccina and koeurax do not yet. The newest `desk` release is `f997e9baf436` (2026-11-08, from `89cc9eb`: `/dev/uhid` and the other static nodes, AX211 Bluetooth firmware, packages' firmware), on koeurax; no `base` went with it. The next `desk` release, which the 2026-11-10 audit run should make with expat and CEF (nothing is published for these alone before it, Joshua, 2026-11-08), carries Minecraft's plans and source packs (R13: its packs and heads are on the origin, its plans are not), `pkg:openssl`'s new pack, and `ab0a7dc`'s firmware report, which the update after that release is the first to use. Gap: a changed `svc` spec goes by hand.
   Next: the key on buccina; Sola's publish runs `oath publish` (its branch
   `oath-heads`, sandbox builds, waits on Joshua's merge); then slices 2 (packaged services) and 4
   (Oath's own heads).
2. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `0c99a6a5` (on CEF 156.0.15):
   core `pkg:sola`, fifteen `pkg:sola-<app>` (`pkg:sola-oath`, the read-only package app, new), `svc:sola-vault`;
   until a machine runs R72, a pin moves PID 1's hash and takes a reboot. R71: `host:local.seat_hid` gives the desk a listed HID device's vendor interface (Sola's mouse battery); novus has `["3554:f510"]` (generation 99). Novus runs the release's Sola builds but `pkg:sola-browser` (next@33d869a8, kept: a plain update drops it). Sola's publish builds `plan:sola-build` with `--no-pkg`; the object is gone from novus and canto. Each machine reboots into it,
   then sets `keep_children` on `svc:sola-session` by hand. Canto: `sola-lock`, Settings →
   Login, Exit Sola. Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)): the 64-bit client runs; on koeurax Joshua played Terraria and Noita on 2026-11-08, and it sees an Xbox pad over Bluetooth LE. Minecraft (R74, [docs/specs/minecraft.md](docs/specs/minecraft.md)): on koeurax since 2026-11-08 (heads on the origin, in Sola's launcher); Joshua ran it that day, the restored 1.21.5 Fabric world's instance on the RTX 4060. Next: Steam's smoke on novus (Vulkan, Floating Point, Proton, a pad).
3. **Servers from source (R49–R59).** Blocky, `cloudflared`, restic, rsync, PostgreSQL with
   PostGIS, our own `route` and `cell`, on `pkg:go`, with `pkg:certs` for roots; in no
   profile (R52), on the origin since `c9735fc14e50` (`cloudflared` since 2026-11-03). They run as `work`, which has no `sudo`. Bias against
   packages, intake gates (R57). PostgreSQL's real test is elsewhere, not on buccina.
   What could still go: [specs/dependencies.md](docs/specs/dependencies.md). `cell` ran
   as `work` under a `svc` on buccina, 2026-11-02. Next: a sandbox for project installs;
   image builds if asked.
4. **Security audit (R48).** `svc:audit` runs on buccina (`0 0 * * *`): scans, reads each pin
   against upstream's newest release, may publish, and puts findings on the origin (R61). Left by Joshua's word: libsndfile, the
   toolchain rebuilds, flex, QEMU's 9p. Linux 7.3.1 carries 12 io_uring patches, out at the first 7.3.y with the series;
   CVE-2026-99255 and 99262 wait for that 7.3.y. rsync 3.5.2 (R67) is on the origin; its fix bounds only unbroken collision runs.
   R67: the audit pins a key upstream names (not yet run for real) and rebuilds CEF itself: on 2026-11-07 it moved CEF to 156.0.26
   with Chromium's fixes for CVE-2026-121035 and 121203, and fribidi 1.0.18, vulkan-loader and vulkan-tools 1.4.367 (desk `bda181db1fc6`,
   then `6c8d38bd54c4`). 44 plans, CEF's too, still name the old `pkg:openssl` through `pkg:python` (2026-11-08): the audit's every-run check (R75) rebuilds them to the same products, CEF with expat's rebuild. Next: expat 2.10.0 at the 2026-11-10 run, a week old (Joshua, 2026-11-04); buccina took `6c8d38bd54c4` at the 2026-11-08 run. R60 is on buccina and canto.

**State:** phases 1–7 are done (6 on 2026-10-27, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). Left until needed: root SSH, more NICs, node rules.

5. **`writ`, Oath's coding agent (R64–R70, [docs/specs/writ.md](docs/specs/writ.md)).** A model works on a project in a box: `writ` is a session on a terminal, which loads the local model as `work`
   (`/llama stop` frees it) or reaches one on OpenRouter (`writ key`, `/use openrouter AUTHOR/NAME`, R70); its writes wait for `/diff`, `/apply`, `/undo`. Overlays on buccina (`b4f6043`) and novus (`5c4bce3`, from the origin; a 40% GPU share, no key yet). Joshua has used it on Kirk, and after a fix to its prompt it did well; Qwen3.5 9B has also reported work falsely, so read its diffs, not its summaries. OpenRouter works: on 2026-11-05 DeepSeek V4.1 Flash did real work for fractions of a cent, 86–98% cached, and six other families did a planted bug. Next: long use, since no real rate limit or failed stream has been met. Qwen3.8 27B now fits beside the desk: on 2026-11-05 Bonsai `Q2_0` (9.2 GiB on the card) and ISTA `IQ3_XXS` (11.5, the cleanest) each did a real fix twice at 17–20 tokens/s, which Qwen3.5 9B did not finish either time; ISTA's larger `IQ3_S` and unsloth's `UD-Q4_K_M` did no better. Drafting with the model's MTP layer made each file about 1.5× faster, and `writ use local --mtp` keeps it on: since 2026-11-05 novus's local model is `IQ3_XXS-mtp` with it (Joshua's choice: 23.8 tokens/s, the real fix in 5.1 minutes, 12.5 GiB), and a session's first lines say `MTP drafting on`. The first prompt after a load still waits about 100 s. The other trial files are deleted.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `06e0194eb4f3` (2026-11-07, the boot screen's status line, unpublished, from a local origin; generation 457; key `oath@novus` in its publishers), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `6c8d38bd54c4` (generation 98, booted 2026-11-07; `pkg:sola-browser` an overlay from Sola's next@33d869a8; key `oath@novus` in its publishers), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `6c8d38bd54c4` (the 2026-11-08 audit's update), generation 176, headless: `desk`'s packages with its services off, Minecraft's 13 packages (`oath publish` publishes a machine's own pin), `svc:audit`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |
| dulcia | `ssh dulcia` from buccina or novus (`home@10.0.0.169`, key in `build/known_hosts`) | OptiPlex 5080, i7-10700T, 64 GB; a server on `base`, installed 2026-11-06 from the stick; `base` release `ee3e239caa32` (generation 6, novus's key scoped to Sola, Mountain time); [hosts/dulcia.md](hosts/dulcia.md) |
| koeurax | `ssh koeurax` from buccina (`home@10.0.0.156`, host key not pinned) | Kora's desk: ASUS B760-I, i5-13400, RTX 4060 on its GSP firmware, Intel AX211 Bluetooth; on Oath since 2026-11-07; `desk` release `f997e9baf436` (booted 2026-11-08), generation 5 with Minecraft's packages; changed only at Joshua's word; [hosts/koeurax.md](hosts/koeurax.md) |

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

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) · [docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) · [docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
