# Plan — Omarchy session payload (Hyprland first)

**Date:** 2026-09-10
**Status:** open (catalog + pack this slice)
**Freeze:** [../specs/2026-09-10-omarchy-session.md](../specs/2026-09-10-omarchy-session.md)

- [x] T39 freeze: session payload, keep Sola, `host:local.session`,
      exclusive compositor, confirm switch, apply+reboot v0.
- [x] `host:local.session` schema + seed `sola`; apply fans out svc
      flags; mutex; `--confirm`; undo restores both.
- [x] PID 1 honors `session` (do not start the other desk).
- [x] `pkg:hyprland` + `svc:hyprland` (enabled false); relocate nixpkgs
      Hyprland `withSystemd = false` like River (pack verified).
- [ ] Probe: pkg present, session sola, hyprland svc off. Sola desk
      still the graphical default (`cargo make build` + `probe`).
- [ ] Next (not this plan): Hyprland paints; then Quickshell +
      `$OMARCHY_PATH`; then DRM-wait live switch.
