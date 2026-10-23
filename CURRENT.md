# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-22 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R28 in [docs/decisions.md](docs/decisions.md)

## Now

1. **eMudhra CA compromise: respond within days (Joshua, 2026-10-20).**
   Eight eMudhra intermediate CA keys leaked
   ([emudhra.we-have-a-problem.com](https://emudhra.we-have-a-problem.com/)).
   `pkg:curl`'s bundle (`cacert-2026-09-25.pem`) has their cross-signer,
   emSign ECC Root CA - G3, so curl, git, `oath`'s fetches and
   `SSL_CERT_FILE` users accept the bad chain (canto: 200 from the
   page's test host). Packs and sources are sha256-pinned; Sola's
   browser refuses it. Next: `pkgs/curl` to `cacert-2026-10-20.pem`
   (`796f1389bfea13c0ed78f663b765baaff54a508e5483786bd6ece73c5e3e3cbe`),
   NSS 3.131.1; build, check, publish, update canto; test Claude Code.
2. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22, [docs/specs/desk.md](docs/specs/desk.md)): rivulet, seatd,
   Xwayland, Sola's session as `svc:sola-*` (terminal, Projects,
   browser, Design), PipeWire, Noto, `svc:sola-kvm`, Vulkan (RADV, the
   compositor's GPU first). Shells outlive a session restart; tabs come
   back after a reboot. Next: realtime priority for PipeWire.
3. **Sola packages itself (R28).** No fork: `desk.json` pins
   `pkg:sola`, `sola-browser` and `sola-design` to Sola's published
   packs, now its `next` at `c53b18e4` (Projects, RADV's Mesa); the
   origin has every pack they need. Oath's side is generic: `oath
   store`, services that follow their package, pinned packs. canto
   keeps Sola's dev pins. Next: new pins when the Sola tab publishes;
   drop `pkg:cef`'s `sola/cef-<version>` link when `pkg:cef` changes.
4. **Claude Code as a recipe (R23).** Every catalog carries
   `plan:claude-code` (stable 2.1.292, pinned to the signed manifest);
   canto built the build host's exact pack and runs `claude`, signed
   in. Next: a new stable is a new pin in `pkgs/claude-code`, then
   `cargo make update`.
5. **Packs from anywhere (R25).** Machines fetch from
   `host:local.origins` in order (a pkg's `url` wins); the default R2
   bucket, `https://store.oath.moonlight.pm`, is written only by `cargo
   make publish` with the build host's key. Next: publish after every
   build that changes a pack; sign `INDEX.md` when a second publisher
   or mirror appears.

**State:** phases 1–5 and 7 are done; 6 (buccina, parked by Joshua:
[hosts/buccina.md](hosts/buccina.md)) and 8 are active
([docs/roadmap.md](docs/roadmap.md)). canto runs `6e4fc1b59fae`
(`desk`), the newest checked output (all 56 QEMU cases), published to
the origin. Left until
something needs them: root SSH as a field, more NICs, device rules as fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine; output `6e4fc1b59fae`, the `desk` profile (rivulet and Sola on DP-10); `ssh novus` works from it; [hosts/canto.md](hosts/canto.md) |
| buccina | 10.0.0.149, no working key | microserver; Wicket trial image, not reinstalled; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS). Not an Oath machine.

## Do not

- `cargo make install --confirm` (wipes a disk) on anything but canto
  unless Joshua asks. Canto is the test machine: install, update,
  reboot and wipe it without asking (Joshua, 2026-10-18).
- `cargo make update --target` (writes a real machine's ESP and store)
  on anything but canto unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS as if it were Oath.
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
