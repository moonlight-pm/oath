# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-18 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R19 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Phase 8 slice 2: the desk.** River is ported to C as rivulet
   (R19, `apps/rivulet/`, 16k lines) and matches upstream 0.4.5
   headless on novus under unmodified `sola-river`, sanitizer-clean
   ([apps/rivulet/README.md](apps/rivulet/README.md)). Next: its
   libraries as plans (Wayland, libdrm, libevdev, libinput,
   libudev-zero, xkbcommon, pixman, libdisplay-info, Mesa with ACO,
   wlroots with `forks/wlroots`' patches, seatd, Xwayland), each
   upstream's signing key added by Joshua; then `pkg:rivulet`, the
   seat, and Sola with oath-sola starting `rivulet`.
2. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, a
   `cargo make install --usb` stick, or `cargo make update` once it
   runs an Oath base.

**State:** phases 1–5 and 7 are done; phases 6 (buccina left) and 8
are active ([docs/roadmap.md](docs/roadmap.md)). On 2026-10-18 haiku
passed all five agent tasks in QEMU on `218fd2e1e03b`, which finished
phase 7 (R16). On 2026-10-17 Oath became a GCC, glibc and GNU system
(R17, R18: phase 8 slice 1; canto reinstalled 2026-10-18).
On 2026-10-16 every target in the
specs short of phase 8 was built: the spec/status split (R14), source
beside each pack on origins (R13), probation with automatic rollback,
store integrity (`oath verify`), schemas generated from the types,
cycle reasons and reverse-`wants` shutdown, DHCP renewals in actual,
device discovery and hotplug, PID 1 surviving its own panic, power
changes without a generation, install's build-host guard and
`cargo make update`. The screen is a `home` shell when no desk owns it
(R15). The newest output, `218fd2e1e03b`, passes all 48 QEMU cases
and, on canto, all 37 metal cases. Left by design until
something needs them: root SSH as a field, more NICs, device rules as
fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013, the test machine; output `218fd2e1e03b` (GCC, glibc), screen is a `home` shell; [hosts/canto.md](hosts/canto.md) |
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
