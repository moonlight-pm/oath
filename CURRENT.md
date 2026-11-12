# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-11-11 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R84 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Packages on their own (R72), PID 1 live (R73).** `oath publish pkg:<name>` puts this
   machine's pin up as its head (`pkg/<name>.json`, signed `oath-pkg`), and `oath update` moves
   each package to the newest head a key in `host:local.publishers` may sign for its name
   (`names="sola,sola-*"` scopes a key: heads only, no release); a need moves only when the package
   keeps every library it loads, else the dependency waits and says why. Seed profiles are data in
   `/lib/oath/seed`, so a pin no longer moves PID 1, and `oath update` re-execs PID 1 into the
   output's binary with every service running; a kernel, PID 1's boot work and the first update from
   a pre-R73 PID 1 still reboot. Released 2026-11-07 from `7597cc1`: `desk` `6c8d38bd54c4` (Sola
   `0c99a6a5`; the audit's `bda181db1fc6` had R72 and R73 too), on buccina, and `base`
   `ee3e239caa32`, on dulcia; canto runs its unpublished successor `06e0194eb4f3`. novus, canto, dulcia and buccina trust key `oath@novus` for any package's head (`names="*"`, Joshua 2026-11-10: either machine publishes any package; releases stay buccina's);
   koeurax, offline, does not yet. The newest releases are `desk` `e3ea93e48cf0` and `base` `7d287d027a91` (2026-11-11, from `d3fc2c0`; QEMU 76/76 and 71/71), `desk` on buccina since 23:15Z (generation 188, PID 1 re-executed, no reboot): the 2026-11-11 audit's CEF fix for CVE-2026-125350 (exploited) and CVE-2026-125346, git 2.56.1, Go 1.27.4 and Xwayland 24.1.15, which its runner did not land because this checkout's master was behind origin, and the third round of code fixes below. Before them, `desk` `3e66f344ba09` and `base` `6d44766bdc26` (2026-11-10, from `070b6b4`): the manual audit's code fixes. Before those, `desk` `84cbc1a7ff81` and `base` `4f32ebdf39cd` (from `a7ceeae`), `desk` on novus since 17:14Z (generation 133, PID 1 in place, no reboot): Claude Code 2.1.311 and `oath add`'s and `oath update --record`'s work below; before those, `desk` `97ae5e337869` (2026-11-09: R78, R81, Sola `cf0e2d40` with `pkg:sola-kvm`) and `base` `c74badbab478`; koeurax runs `desk` `f997e9baf436`. They carry `pkg:certs` in `base`, recipe plans in the manifest, Minecraft's plans and source packs (R13), `pkg:openssl`'s new pack, and `ab0a7dc`'s firmware report, which the update after them is the first to use. The 2026-11-12 audit run should make the next `desk` with expat, harfbuzz, libsndfile's fix and CEF. Gap: a changed `svc` spec goes by hand.
   R81: `oath add <name>` and `oath ls --origin` install by name with dependencies, from a signed head for every package but `linux`, `oath` and `firmware`; released 2026-11-09, in the manual. Released 2026-11-10: `oath add` records the heads it took and, run as `home`, says what it would add and each pack's size; `sudo oath update --record --confirm` records the heads a machine runs, moving nothing, which a publisher runs after `oath publish` (Sola's request, 2026-11-09: novus's `oath get` called its own published heads overlays); buccina recorded its 155 that day. `oath add` also refuses a `/bin` or `/lib` clash before changing anything and undoes an apply that stops part way, and no package links `/lib/python`, so the pure-Python build packs install side by side (novus's half `oath add python-mako`, generation 129, was undone 2026-11-10). novus runs it and has recorded its heads, so its `oath diff` is in sync with no overlays (2026-11-10). The manual audit's code fixes (2026-11-10) are in those releases: PID 1 runs only applied services, keeps a finished `on-failure` one down and starts every boot with the desk; a machine-id each; `oath build` moves no pin and runs every published plan on a machine; `verify` and apply repair a missing or damaged tree; `net:net0.dns` and POSIX-only `timezone` (R83); `snap:current` refused; apply hashes a present tree once; `oath add` takes no head older than one taken; `pkg:postgis` in `lib/postgis` without its Perl scripts (`sha256-4b8640af…`); and, as its head rather than in the outputs, `pkg:writ` `b07626b9…` with writ's `run --session`, check, and apply and undo of links and deletions; oath-make's install, esp, check and QEMU. The manual describes them. A third round is in the newest releases: an update holds an older head for a package new to the machine or the release, as for one it has; a damaged or missing cell drifts in `tree` and apply refuses it before a generation; the INDEX and `oath schema` name each damaged tree's repair; PID 1 makes the machine-id on a re-exec too; a static `dns`-only apply leaves the address and an ignored `ip` step is quiet; writ's undo resumes where it stopped and leaves what the owner put there (`pkg:writ` `923e124d…`); an update makes a package new to the profile though nothing here needs it. On master since 2026-11-11, not released: a head that moves only its needs is taken; an update reads the heads of what the release makes and makes a package new to the profile present at its head; `oath store` of a damaged tree clears its damage; undo checks a cell's tree first; `oath ls <kind>`; the build sandbox's packs are read-only in a user namespace too (the audit's box), so the held cascade (expat, libsndfile, harfbuzz, Vulkan, OpenSSL) can go ahead; writ's undo stops at what it cannot remove and keeps what the owner kept.
   Next: Sola's publish runs `oath publish` (its branch `oath-heads`, sandbox builds, waits on Joshua's merge); then slice 2 (packaged services) and the rest of 4 (the kernel and PID 1 as heads).
2. **Phase 8 slice 2: the desk** (R19–R22, R28, R37, R40, R42,
   [docs/specs/desk.md](docs/specs/desk.md)) on canto and novus. R44:
   novus's `sola-kvm server` drives canto (`svc:sola-kvm` off in the
   profile). R47: the profile pins Sola master `cf0e2d40` (on CEF 156.0.26):
   core `pkg:sola`, `pkg:sola-kvm` (the KVM and Remote Desktop, split from the core, which needs it), fifteen `pkg:sola-<app>`, `svc:sola-vault`;
   until a machine runs R72, a pin moves PID 1's hash and takes a reboot. R71: `host:local.seat_hid` gives the desk a listed HID device's vendor interface (Sola's mouse battery); novus has `["3554:f510"]` (generation 99). Novus runs exactly these heads, applied by Sola's publish of 2026-11-10 01:44Z. Sola's publish builds `plan:sola-build` with `--no-pkg`; the object is gone from novus and canto. Each machine reboots into it,
   then sets `keep_children` on `svc:sola-session` by hand. Canto: `sola-lock`, Settings →
   Login, Exit Sola. Steam (R29, [docs/specs/steam.md](docs/specs/steam.md)): the 64-bit client runs; on koeurax Joshua played Terraria and Noita on 2026-11-08, and it sees an Xbox pad over Bluetooth LE. Minecraft (R74, [docs/specs/minecraft.md](docs/specs/minecraft.md)): on koeurax since 2026-11-08 (heads on the origin, in Sola's launcher); Joshua ran it that day, the restored 1.21.5 Fabric world's instance on the RTX 4060.
   Sleep (R84, 2026-11-11, Sola's request for koeurax): `host:local.power=sleep` on the desk's reboot path, woken by a key, a mouse, the power button or, with `net:net0.wake`, a magic packet (`oath wake <mac>`); NVIDIA's `PreserveVideoMemoryAllocations=1` is PID 1's default. On master and checked in QEMU, not released or tried on metal; Sola wires it on its branch `quick-launch-sleep`. Next: a release (Joshua's word), koeurax's update with a reboot, `wake=true` there, then a sleep woken from novus; Steam's smoke on novus (Vulkan, Floating Point, Proton, a pad).
3. **Servers from source (R49–R59).** Blocky, `cloudflared`, restic, rsync, PostgreSQL with
   PostGIS, MySQL 9.7, our own `route` and `cell`, on `pkg:go`, with `pkg:certs` for roots; in no
   profile (R52), on the origin since `c9735fc14e50` (`cloudflared` since 2026-11-03). They run as `work`, which has no `sudo`. Bias against
   packages, intake gates (R57). PostgreSQL's real test is elsewhere, not on buccina.
   R82: `pkg:mysql` is its server and clients, and `pkg:aws` is AWS CLI v2 on `pkg:python` with `session-manager-plugin`; both ran on buccina 2026-11-09, with no real AWS sign-in or database yet, which is Joshua's test.
   What could still go: [specs/dependencies.md](docs/specs/dependencies.md). `cell` ran
   as `work` under a `svc` on buccina, 2026-11-02. `cell run pkg:<name>` (R76, [specs/cells.md](docs/specs/cells.md)) runs an installed package with no image, its packs bound in read-only; `cell:<name>` (R78), a ninth kind, is one project's own software, stored apart, never linked or published. Both released 2026-11-09 (`pkg:cell` `7d68620c6f40`, in no profile: `sudo oath add cell` installs it with what it needs); the manual has a chapter on cells. Images and both of these run side by side. Next: Joshua tries it with a project on novus; from that, a project's source to a `cell:`, built by `home` on one machine and run on another (novus to dulcia), porting dulcia's workloads one by one; a sandbox for project installs.
4. **Security audit (R48, R77).** `svc:audit` runs on buccina (`0 0 * * *`): the agent, in a box (`check/audit/box`) with no key but `~/.config/oath/claude-token` (made 2026-11-09, good for a year; a run without it mails that it did not run), scans, reads each pin
   against upstream's newest release, fixes and checks; the runner, outside, lands its commits, pushes, publishes, and puts findings on the origin (R61). Left by Joshua's word: libsndfile but for R77's one fix, the
   toolchain rebuilds, flex, QEMU's 9p. Linux 7.3.1 carries 12 io_uring patches, out at the 2026-11-14 run with 7.3.2 or newer,
   which fixes CVE-2026-99255 and 99281. rsync 3.5.2 (R67) is on the origin; its fix bounds only unbroken collision runs.
   R67: the audit pins a key upstream names (not yet run for real) and rebuilds CEF itself: on 2026-11-07 it moved CEF to 156.0.26
   with Chromium's fixes for CVE-2026-121035 and 121203, and fribidi 1.0.18, vulkan-loader and vulkan-tools 1.4.367 (desk `bda181db1fc6`,
   then `6c8d38bd54c4`). The 2026-11-09 run rebuilt 43 of the 44 plans that named the old `pkg:openssl` (R75) and only CEF is left. The 2026-11-11 run rebuilt CEF with upstream's fix for CVE-2026-125350, exploited in the wild, and moved git, Go and Xwayland; they went out in `e3ea93e48cf0`. It holds expat, libsndfile's fix, harfbuzz, Vulkan and OpenSSL until the build sandbox can make the store read-only, a fix in `crates/` (its report). Next: bluez 5.88 at the 2026-11-13 run; CEF 157 brings zlib 1.4 and curl's bundle onto `pkg:certs` (R50, R77). Qt waits for 6.12.1. buccina took `6c8d38bd54c4` at the 2026-11-08 run. R60 is on buccina and canto.

**State:** phases 1–7 are done (6 on 2026-10-27, [hosts/buccina.md](hosts/buccina.md)); 8 and 9
are active ([docs/roadmap.md](docs/roadmap.md)). Left until needed: root SSH, more NICs, node rules.

5. **`writ`, Oath's coding agent (R64–R70, [docs/specs/writ.md](docs/specs/writ.md)).** A model works on a project in a box: `writ` is a session on a terminal, which loads the local model as `work`
   (`/llama stop` frees it) or reaches one on OpenRouter (`writ key`, `/use openrouter AUTHOR/NAME`, R70); its writes wait for `/diff`, `/apply`, `/undo`. Overlays on buccina (`b4f6043`) and novus (`5c4bce3`, from the origin; a 40% GPU share, no key yet). Joshua has used it on Kirk, and after a fix to its prompt it did well; Qwen3.5 9B has also reported work falsely, so read its diffs, not its summaries. OpenRouter works: on 2026-11-05 DeepSeek V4.1 Flash did real work for fractions of a cent, 86–98% cached, and six other families did a planted bug. Next: long use, since no real rate limit or failed stream has been met. Qwen3.8 27B now fits beside the desk: on 2026-11-05 Bonsai `Q2_0` (9.2 GiB on the card) and ISTA `IQ3_XXS` (11.5, the cleanest) each did a real fix twice at 17–20 tokens/s, which Qwen3.5 9B did not finish either time; ISTA's larger `IQ3_S` and unsloth's `UD-Q4_K_M` did no better. Drafting with the model's MTP layer made each file about 1.5× faster, and `writ use local --mtp` keeps it on: since 2026-11-05 novus's local model is `IQ3_XXS-mtp` with it (Joshua's choice: 23.8 tokens/s, the real fix in 5.1 minutes, 12.5 GiB), and a session's first lines say `MTP drafting on`. The first prompt after a load still waits about 100 s. The other trial files are deleted.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine and Joshua's Sola R&D desk (no long Oath builds); output `06e0194eb4f3` (2026-11-07, the boot screen's status line, unpublished, from a local origin; generation 461; key `oath@novus` in its publishers for any package), the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` and `ssh buccina` work from it; [hosts/canto.md](hosts/canto.md) |
| novus | `ssh home@10.0.0.5` (key in `build/novus-oath.known_hosts`) | Joshua's desk: Ryzen 9 5950X, RTX 3090 Ti; installed 2026-10-30 by `oath install`, output `84cbc1a7ff81` (the 2026-11-10 release, generation 133, booted 2026-11-09; key `oath@novus` in its publishers for any package), `/home` on its 2 TB, the KVM server for canto, `desk` with `pkg:nvidia`; `ssh buccina` works from it; ask before changing it (R32); [hosts/novus.md](hosts/novus.md) |
| buccina | `ssh home@10.0.0.149` (key in `build/known_hosts`) | OptiPlex 5000 Micro, i7-12700T, 64 GB; where Oath work happens (R39): checkout, builds, publishing; output `e3ea93e48cf0` (2026-11-11, PID 1 re-executed, no reboot), generation 188 (key `oath@novus` in its publishers for any package), headless: `desk`'s packages with its services off, Minecraft's 13 packages (`oath publish` publishes a machine's own pin), `svc:audit`; `ssh canto` works from it; [hosts/buccina.md](hosts/buccina.md) |
| dulcia | `ssh dulcia` from buccina or novus (`home@10.0.0.146`, key in `build/known_hosts`) | OptiPlex 5080, i7-10700T, 64 GB; a server on `base`, installed 2026-11-06 from the stick; `base` release `ee3e239caa32` (generation 64, novus's key for any package, Mountain time; Wicket's services and workloads); [hosts/dulcia.md](hosts/dulcia.md) |
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
