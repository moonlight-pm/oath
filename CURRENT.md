# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-20 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R27 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Phase 8 slice 2: the desk.** canto runs the `desk` profile
   (R19–R22): rivulet on DP-10 (radeonsi on ACO), seatd, Xwayland;
   Sola's session as `svc:sola-*` with the terminal, Workspaces and the
   browser (CEF 147); PipeWire, Noto, Sola's build tools, `svc:sola-kvm`
   ([docs/specs/desk.md](docs/specs/desk.md)). The tmux servers are
   `svc:sola-tmux` and `svc:sola-tmux-ws`, so shells outlive a session
   restart, and the terminal rebuilds its saved tabs after a reboot
   (two Sola-generic commits in the fork, not yet on Sola `master`).
   Next: Sola Design on canto (Joshua, 2026-10-20); realtime priority
   for PipeWire.
2. **Claude Code as a recipe (R23).** Every catalog carries
   `plan:claude-code` (stable 2.1.292, pinned to the signed manifest);
   canto built the build host's exact pack and runs `claude`. Next:
   Joshua signs in on canto; a new stable is a new pin in
   `pkgs/claude-code`, then `cargo make update`.
3. **Packs from anywhere (R25).** A pack records no location; the
   machine looks in `host:local.origins` in order (a pkg's `url` wins)
   and logs where a tree came from. Default origin: the R2 bucket
   `oath-store` at `https://store.oath.moonlight.pm`, public read,
   written by `cargo make publish` with the build host's key
   (`~/.config/oath/origin.env`): every locked pack, build packs too,
   with plan and source (404 files, 6.24 GB on 2026-10-19). The host
   store is `~/.local/share/oath/store`; Wicket's old `oath-store`
   workload is deleted. Next: publish after every `cargo make pkgs` or
   build that changes a pack; signatures on `INDEX.md` when a second
   publisher or mirror appears.
4. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, a
   `cargo make install --usb` stick, or `cargo make update` once it
   runs an Oath base.

**State:** phases 1–5 and 7 are done; 6 (buccina left) and 8 are
active ([docs/roadmap.md](docs/roadmap.md)). Phase 7 finished
2026-10-18 (haiku passed all five agent tasks, R16); Oath became a GCC,
glibc and GNU system 2026-10-17 (R17, R18). The newest checked output,
`64c80ede5e5b` (`desk`), passes all 53 QEMU cases, and canto runs it;
canto passed all 42 metal cases on `f897ffcbda55` (`base`, zsh as
`home`'s login shell, R27) and `desk-sola` on `64c80ede5e5b`. Left by design until something needs them: root SSH as a field,
more NICs, device rules as fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine; output `64c80ede5e5b`, the `desk` profile (rivulet and Sola on DP-10); `ssh novus` works from it; [hosts/canto.md](hosts/canto.md) |
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
