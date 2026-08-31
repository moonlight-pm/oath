# Plan — Sola on Oath, River first

**Date:** 2026-08-30
**Status:** complete (River/seatd). `oath-sola` is not this plan.
**Proof:** probe `river.running` / `river.wayland` (2026-08-30).
**Freeze:** [../specs/2026-08-30-sola.md](../specs/2026-08-30-sola.md)
**Next:** [2026-08-30-libinput-plan.md](2026-08-30-libinput-plan.md)

- [x] T21 freeze + `forks/` layout lock.
- [x] GitHub `oath-river` + `oath-wlroots` (Sola patches as commits).
      `oath-sola` still not created.
- [x] Submodule `forks/river` + `forks/wlroots`.
- [x] Seed sealed `pkg:glibc`; relocate glibc + River ELF deps.
- [x] Pack patched `river` as `pkg:river`; `svc:river` execs `/bin/river`.
- [x] `svc:seatd`; pixman; do not patchelf `ld-linux`. Probe: stable
      pid + Wayland socket. No udev/libinput.
