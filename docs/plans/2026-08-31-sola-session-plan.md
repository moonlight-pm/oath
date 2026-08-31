# Plan — Sola session manager on Oath

**Date:** 2026-08-31
**Status:** complete
**Proof:** probe `sola.session` (plus T23 `sola.bus` / `call` / `bridge` /
`shell` / `sola.no_pm`). gtk 1280×800 1:1 is host QEMU, not this probe.
**Freeze:** [../specs/2026-08-31-sola-session.md](../specs/2026-08-31-sola-session.md)

- [x] T25 freeze: `svc:sola-session` in `pkg:sola`; no nested PM; dual-mode
      logs / Wayland / `/bin`.
- [x] Pack `sola-session` as fifth ELF; seed `svc:sola-session` (`wants`
      bus + call, `restart=on-failure`).
- [x] Dual-mode: `/oath/log` capture; live Wayland for children; launcher
      builtins `/bin` on Oath.
- [x] QEMU: virtio-gpu `xres`/`yres` 1280×800 + gtk `zoom-to-fit=off`
      (guest matches window; `OATH_DISPLAY_WIDTH` / `HEIGHT`).
- [x] Probe `sola.session`. Tests. Manual.
