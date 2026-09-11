# Plan — Omarchy session payload (Hyprland first)

**Date:** 2026-09-10
**Status:** complete (catalog + Hyprland + Quickshell bar; remaining
gaps live on the freeze)
**Freeze:** [../specs/2026-09-10-omarchy-session.md](../specs/2026-09-10-omarchy-session.md)

- [x] T39 freeze: session payload, keep Sola, `host:local.session`,
      exclusive compositor, confirm switch, apply+reboot v0.
- [x] `host:local.session` schema + seed `sola`; apply fans out svc
      flags; mutex; `--confirm`; undo restores both.
- [x] PID 1 honors `session` (do not start the other desk).
- [x] `pkg:hyprland` + `svc:hyprland` (enabled false); relocate nixpkgs
      Hyprland `withSystemd = false` like River (pack verified).
- [x] Canto paint: `session=omarchy`, Hyprland on DP-10 1920×1080,
      `wayland-1` (2026-09-11). Busybox tar `../` symlink strip fixed
      in relocate (absolute guest paths).
- [x] Probe: pkg present, session sola default on QEMU image
      (`cargo make build` + `probe` T39 pack facts). Later probe
      steps fail from dbus/pipewire serial spam (not a pack miss).
- [x] Quickshell + `$OMARCHY_PATH`; DRM-wait live switch; ESP rotate
      so PID 1 is T39 init. Canto leftover `sola-hypr=1` /
      `sola-omarchy=1` exec tokens are harmless.

Not this plan (freeze gaps): Hyprland 0.52 vs Omarchy Lua ≥0.56;
session dbus / UPower / SNI / Polkit; Quickshell.Networking /
PwNodePeakMonitor; agent collectors.
