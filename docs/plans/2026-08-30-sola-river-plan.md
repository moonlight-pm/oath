# Plan — Sola on Oath, River first

**Date:** 2026-08-30
**Status:** open (`oath-sola` still out)
**Proof:** probe `river.running` / `river.wayland` (2026-08-30).
**Freeze:** [../specs/2026-08-30-sola.md](../specs/2026-08-30-sola.md)

- [x] T21 freeze + `forks/` layout lock.
- [x] GitHub `oath-river` + `oath-wlroots` (Sola patches as commits).
      `oath-sola` still not created.
- [x] Submodule `forks/river` + `forks/wlroots`.
- [x] Seed sealed `pkg:glibc`; relocate glibc + River ELF deps.
- [x] Pack patched `river` as `pkg:river`; `svc:river` execs `/bin/river`.
- [x] Probe: Wayland socket / `svc:river` running; headless probe
      still passes. Manual + capabilities.
