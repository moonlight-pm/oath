# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-20 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R28 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22): rivulet on DP-10 (radeonsi on ACO), seatd, Xwayland;
   Sola's session as `svc:sola-*` with the terminal, Workspaces and the
   browser (CEF 147); PipeWire, Noto, Sola's build tools, `svc:sola-kvm`
   ([docs/specs/desk.md](docs/specs/desk.md)). The tmux servers are
   `svc:sola-tmux` and `svc:sola-tmux-ws`, so shells outlive a session
   restart, and the terminal rebuilds its saved tabs after a reboot.
   Sola Design runs there too (R21, 2026-10-20). Next: realtime
   priority for PipeWire.
2. **Sola packages itself (R28).** `forks/sola` is frozen at
   `aa548c8a`; Sola builds, stores and publishes its own packs. Oath's
   generic side is built (2026-10-20): `oath store`; services restart
   when their program's package moves; a profile can pin a pack built
   elsewhere (`build` fetches it, `publish` skips it, `update` keeps
   the machine's pin and warns on stale needs). Next: with the Sola
   tab's first published hashes, pin `pkg:sola`, `sola-browser` and
   `sola-design` in `desk.json`; delete `pkgs/sola*`, their lock
   entries, `forks/sola` and an unused `cargo_vendor`; fix R21,
   forks/README.md, AGENTS.md, desk.md and builds.md.
3. **Claude Code as a recipe (R23).** Every catalog carries
   `plan:claude-code` (stable 2.1.292, pinned to the signed manifest);
   canto built the build host's exact pack and runs `claude`, signed
   in. Next: a new stable is a new pin in `pkgs/claude-code`, then
   `cargo make update`.
4. **Packs from anywhere (R25).** Machines fetch from
   `host:local.origins` in order (a pkg's `url` wins); the default is
   the R2 bucket at `https://store.oath.moonlight.pm`, written only by
   `cargo make publish` with the build host's key
   (`~/.config/oath/origin.env`). Next: publish after every build that
   changes a pack; signatures on `INDEX.md` when a second publisher or
   mirror appears.
5. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, a `--usb` stick, or
   `cargo make update` once it runs an Oath base.

**State:** phases 1–5 and 7 are done; 6 (buccina left) and 8 are
active ([docs/roadmap.md](docs/roadmap.md)). The newest checked output,
`6b16187831bc` (`desk`: `oath store`, services that follow their
package), passes all 55 QEMU cases; canto runs its userspace. Left by
design until something needs them: root SSH as a field, more NICs,
device rules as fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine; output `6b16187831bc`, the `desk` profile (rivulet and Sola on DP-10); `ssh novus` works from it; [hosts/canto.md](hosts/canto.md) |
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
