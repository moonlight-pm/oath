# CURRENT

The only session handoff. Keep it under 80 lines: at most five Now
items, each ending in the next concrete step. Replace text; do not
append history (git has it).

**As of:** 2026-10-17 · **Review:** [briefing](https://claude.ai/artifact/PoPUy1UxBpQdcuPBkDonTL)
· decisions R1–R18 in [docs/decisions.md](docs/decisions.md)

## Now

1. **Phase 8 slice 1: GCC, glibc and the GNU userland (R17, R18).**
   Built: GCC 16.2, binutils 2.47 and glibc 2.44 bootstrapped from the
   host's gcc; 51 locked packages, shared libraries through a `/lib`
   farm (no `/lib64`), each pack's `needs` from the libraries it loads
   (checked after every build); bash, coreutils and the GNU tools,
   dhcpcd, chrony, vim; busybox, Clang/LLVM and musl gone. Output
   `29b1b9a54be1` passes all 48 QEMU cases. Next: reinstall canto
   (Joshua approved 2026-10-17; an update cannot cross this change),
   run its `metal` cases and mount its NFS backup target once.
2. **Phase 7, agent surface (R16).** `oath mcp` and
   `cargo make check --agent` are built
   ([docs/specs/agent.md](docs/specs/agent.md)); canto passed all five
   agent tasks with haiku on `e3a11645f14c`. The key in this session's
   environment is refused, so Joshua runs it. Next: `cargo make check
   --agent` in QEMU on `29b1b9a54be1`; when all five pass, phase 7 is done
   (flip it in the roadmap).
3. **Phase 8 slice 2: the desk.** Mesa (ACO, no LLVM), Wayland,
   wlroots and Sola as plans. Open first: River is Zig, which R9 rules
   out ([docs/decisions.md](docs/decisions.md)).
4. **buccina (phase 6), parked by Joshua.** It answers at 10.0.0.149
   with a new host key and refuses the build host's key as root. Next,
   when Joshua picks it up: the Wicket deploy key, a
   `cargo make install --usb` stick, or `cargo make update` once it
   runs an Oath base.

**State:** phases 1–5 are done; phases 6 (buccina left), 7 and 8 are active
([docs/roadmap.md](docs/roadmap.md)). On 2026-10-17 `oath mcp` and the
real-model check were built (R16), and Oath became a GCC, glibc and
GNU system (R17, R18). On 2026-10-16 every target in the
specs short of phase 8 was built: the spec/status split (R14), source
beside each pack on origins (R13), probation with automatic rollback,
store integrity (`oath verify`), schemas generated from the types,
cycle reasons and reverse-`wants` shutdown, DHCP renewals in actual,
device discovery and hotplug, PID 1 surviving its own panic, power
changes without a generation, install's build-host guard and
`cargo make update`. The screen is a `home` shell when no desk owns it
(R15). The newest output, `29b1b9a54be1`, passes all 48 QEMU cases;
canto's, `e3a11645f14c`, all 37 metal cases. Left by design until
something needs them: root SSH as a field, more NICs, device rules as
fields.

## Machines

| Host | Reach | Notes |
|------|-------|-------|
| canto | `ssh home@10.0.0.3` (key in `build/known_hosts`) | Mac Pro 2013; output `e3a11645f14c` (`oath mcp`), screen is a `home` shell; [hosts/canto.md](hosts/canto.md) |
| buccina | 10.0.0.149, no working key | microserver; Wicket trial image, not reinstalled; [hosts/buccina.md](hosts/buccina.md) |

Build host: **novus** (NixOS). Not an Oath machine.

## Do not

- `cargo make install --confirm` (wipes a disk) unless Joshua asks.
- `cargo make update --target` (writes a real machine's ESP and store)
  unless Joshua asks; `--qemu` rehearsals are fine.
- Write a USB stick or real disk Joshua did not ask for.
- Change novus's OS as if it were Oath.
- Invent answers to open decisions in [docs/decisions.md](docs/decisions.md).

## Pointers

[AGENTS.md](AGENTS.md) · [docs/capabilities.md](docs/capabilities.md) ·
[docs/roadmap.md](docs/roadmap.md) · [docs/specs/](docs/specs/) ·
[docs/manual/](docs/manual/) · [docs/architecture.md](docs/architecture.md)
