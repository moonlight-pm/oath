# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-24 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· **novus:** [tracker](https://claude.ai/artifact/LSm9uH7rrfsWJiEN8p1Egi)
· decisions R1–R34 in [docs/decisions.md](docs/decisions.md)

## Now

1. **eMudhra CA compromise: respond within days (Joshua, 2026-10-20).**
   Eight eMudhra intermediate keys leaked
   ([emudhra.we-have-a-problem.com](https://emudhra.we-have-a-problem.com/));
   `pkg:curl`'s bundle (`cacert-2026-09-25.pem`) trusts their
   cross-signer, emSign ECC Root CA - G3 (packs and sources are
   sha256-pinned; Sola's browser refuses it). Next: `pkgs/curl` to `cacert-2026-10-20.pem`
   (`796f1389bfea13c0ed78f663b765baaff54a508e5483786bd6ece73c5e3e3cbe`),
   NSS 3.131.1; build, check, publish, update canto; test Claude Code.
2. **Steam on canto (R29, [docs/specs/steam.md](docs/specs/steam.md)).**
   Valve's launcher 1.0.0.87, a recipe, in a bubblewrap sandbox; canto
   runs the 64-bit client, signed in, with the i386 stack (`glibc-i386`,
   `mesa-i386` and 15 more). PID 1 gives `home` the pads' nodes (R30)
   from the next boot. Next: a 32-bit game and a pad, after that reboot.
3. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22, [docs/specs/desk.md](docs/specs/desk.md)): rivulet, seatd,
   Xwayland, PipeWire, Noto, Vulkan (RADV), `wl-clipboard` (generation
   319), and Sola's session from Sola's own packs, which `desk.json`
   pins (R28, no fork; its `next` at `c6cda58b`). Next: realtime priority for PipeWire; new pins when the
   Sola tab publishes.
4. **novus to Oath (R32–R35, [hosts/novus.md](hosts/novus.md)).** NixOS
   goes; novus gets its own Oath install. Built: bootstrap on an Oath
   host, `pkg:qemu`, `/dev/kvm` for `home`, novus's drivers (canto runs
   them, `9466271b702e`, reboot pending), and NVIDIA 595.104.02 as a
   recipe in `desk` (modules against `pkg:linux-build`, loaded from the
   package by PID 1; `host:local.module_options`). canto runs
   `bootstrap` then `pkgs` from scratch (`self-host` tab); the lock must
   not change. Next: libglvnd and Mesa with glvnd; novus's trial on a
   USB SSD (Joshua's disk and word).
5. **Recipes and origins (R23, R25, R31).** Every catalog carries the
   coding agents' plans (`claude-code` 2.1.292, `codex` 0.164.0,
   `grok-build` 1.0.70) and `plan:steam`; machines fetch packs from
   `host:local.origins`, the R2 bucket written only by `cargo make
   publish`. Next: a new stable is a new pin; publish after every build
   that changes a pack; sign `INDEX.md` when a second publisher appears.

**State:** phases 1–5 and 7 are done; 6 (buccina, parked by Joshua:
[hosts/buccina.md](hosts/buccina.md)) and 8 are active
([docs/roadmap.md](docs/roadmap.md)). canto runs `9466271b702e`
(`desk`, R32–R35; generation 324), the newest checked output (all 57 QEMU
cases), not yet published; its kernel and PID 1 are `048c0e633cd7`'s
(reboot of 2026-10-24 04:31Z) until the next boot. Left until needed:
root SSH as a field, more NICs, device-node rules as fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine; output `9466271b702e`, the `desk` profile (rivulet and Sola on DP-10) and Steam; `ssh novus` works from it; [hosts/canto.md](hosts/canto.md) |
| buccina | 10.0.0.149, no working key | microserver; Wicket trial image, not reinstalled; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS), to become Oath ([hosts/novus.md](hosts/novus.md)).

## Do not

- `cargo make install --confirm` (wipes a disk) on anything but canto
  unless Joshua asks. Canto is the test machine: install, update,
  reboot and wipe it without asking (Joshua, 2026-10-18).
- `cargo make update --target` (writes a real machine's ESP and store)
  on anything but canto unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS, or write its disks, before Joshua says so (R32).
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
