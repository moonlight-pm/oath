**Date:** 2026-09-10
**Status:** target (freeze)
**Implementation:** partial (catalog session + Hyprland + Quickshell bar on canto)
**Dogfood:** canto `session=omarchy`; Hyprland 0.52.2 on Pitcairn DP-10
  1920×1080 (Philips 221V8L); `wayland-1`; `omarchy-bar` layer 1920×26
  as `home`; River/Sola stopped; ESP last-5 includes T39 initrd (boot 12);
  rebooted onto T39 `/init` (kernel 7.3.0-rc1 #4; Hyprland pid 362,
  omarchy-shell pid 367, restarts=0)
**Gaps:** Hyprland 0.52 (nixpkgs) not Omarchy’s Lua ≥0.56 desk; QEMU
  image packed (8G rootfs) — T39 pack facts probe ok; later probe
  steps fail from dbus/pipewire serial spam; no session dbus / UPower /
  SNI / Polkit; Quickshell 0.2.1 lacks `Quickshell.Networking` and
  `PwNodePeakMonitor`; Omarchy agent collectors need a fuller bash
  userland; libdrm `amdgpu.ids` still a nix store path; Xwayland
  autostarted (SI glamor not smoked)
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Omarchy as a session payload (keep Sola)

Oath stays the OS. Sola stays a desk. Omarchy is a second **session
payload** — Hyprland + (later) Quickshell + the Omarchy tree — packed
as glibc `pkg:*` and started as `svc:*`. The operator picks one desk.
Both sets of bits stay on the machine.

This is not “become NixOS,” not systemd, not a nested process
manager, not a throwaway compositor, and not uninstalling Sola.

---

## Locks this freeze owns

- **No new kind.** The choice is `host:local.session` = `sola` |
  `omarchy`. Default **`sola`**. Same object as hostname / timezone /
  power.
- **Exclusive compositor.** One DRM owner. River and Hyprland must
  not both be enabled. Apply refuses that pair. PID 1 also keys off
  `session`: `omarchy` does not start River or `svc:sola-*`; `sola`
  does not start Hyprland (or later `svc:omarchy-*`).
- **Pack both, run one.** `pkg:sola` / `pkg:river` stay present when
  the desk is Omarchy. `pkg:hyprland` / `pkg:quickshell` / `pkg:omarchy`
  stay present when the desk is Sola. `svc:* enabled` is what runs.
- **Shared seat stays up** across a switch: `svc:seatd`, pipewire
  trio, `svc:dbus` / `bluetoothd`, sshd, net. sola-kvm is Sola-only
  (River virtual pointer).
- **`session` apply is confirm.** Switching desks kills the graphical
  session. `oath apply --confirm`. Undo restores the previous
  session and the svc flags written in that generation.
- **Live switch waits for DRM.** PID 1 SIGTERMs the outgoing
  compositor and waits up to 3 s before starting the incoming one.
  Reboot remains the escape if the race still loses. Installer / seed
  is the same field.
- **PID 1 is the only supervisor.** No UWSM, no SDDM, no user
  systemd, no nested Omarchy/Sola process manager. Hyprland is
  `NO_SYSTEMD` (`withSystemd = false`). Seat is `home` via seatd +
  libudev-zero (no udevd).
- **Compositor + bar.** Objects: **`pkg:hyprland`** / **`svc:hyprland`**
  (`/bin/hyprland`, `wants: svc:seatd`, `restart=always`, seed
  **enabled=false**); **`pkg:quickshell`** / **`svc:omarchy-shell`**
  (`/bin/quickshell`, `wants: svc:hyprland`, `restart=always`, seed
  **enabled=false**); **`pkg:omarchy`** at `$OMARCHY_PATH`. Borrowed
  nixpkgs Hyprland + Quickshell, relocated like River. Omarchy tree is
  fetchFromGitHub (not a `forks/` tree).
- **Sola remains the default seed.** Canto dogfood may run
  `session=omarchy` (Hyprland compositor + Quickshell `omarchy-bar`
  painted 2026-09-11).
- **Two CLIs stay split.** `oath` owns catalog / host / svc / pkg.
  `omarchy` (later) owns theme, bar, capture, launch. `omarchy pkg` /
  `omarchy update` wrap `oath` or are absent. `/bin/xdg-open` stays
  `solactl open` while `session=sola`.
- **Amends T21 “no throwaway compositor.”** River is the Sola
  compositor. Hyprland is the Omarchy compositor. Do not add a
  third.

---

## Courage test (this slice)

1. Seed: `host:local.session` is `sola`; `svc:hyprland` exists and is
   **enabled=false**; `svc:river` and the Sola stack stay enabled.
2. `oath set host:local session=omarchy` then `oath apply` without
   `--confirm` exits 3. With `--confirm`, River + `svc:sola-*` go
   enabled=false and `svc:hyprland` enabled=true. `oath undo` restores
   Sola.
3. `oath set svc:hyprland enabled=true` while River is still enabled
   refuses (one compositor).
4. After `cargo make build`, `pkg:hyprland` is present and
   `/bin/hyprland` is a symlink into the store. QEMU probe checks
   those catalog facts. Probe still boots the **Sola** desk.
5. Serial and SSH still work.

Hyprland **painting** on canto is in. Quickshell `$OMARCHY_PATH/shell`
bar is in (`svc:omarchy-shell`). Omarchy Lua Hyprland ≥0.56 config is
not (packed compositor is 0.52 conf).

---

## Out

- SDDM, UWSM, NetworkManager, pacman/AUR, Limine/Snapper, Plymouth
- `forks/omarchy` until the tree is vendored
- Dual desks on one VT; two seats; udevd; systemd
- Replacing River as the Sola compositor
- Promising Hyprland on canto Pitcairn or virtio-gpu 2D
- home-manager-style immutable `~/.config`
