# Plan — Sola on Oath, River first

**Date:** 2026-08-30
**Status:** open
**Proof:** (none yet)
**Freeze:** [../specs/2026-08-30-sola.md](../specs/2026-08-30-sola.md)

- [x] T21 freeze + `forks/` layout lock.
- [ ] GitHub forks under `moonlight-pm` (`oath-sola`, `oath-river`,
      `oath-wlroots`) — **blocked on creating those remotes**.
- [ ] Replay Sola `nix/patches/` as commits on `oath-wlroots` then
      `oath-river`.
- [ ] Submodule them at `forks/sola`, `forks/river`, `forks/wlroots`.
- [ ] Seed sealed `pkg:glibc` from a borrowed glibc (D8); patchelf
      or RUNPATH for river. Never mix into musl PID 1.
- [ ] Pack patched `river` as `pkg:river`; `svc:river` execs it.
- [ ] Probe: Wayland socket / `svc:river` running; headless probe
      still passes. Manual + capabilities.
