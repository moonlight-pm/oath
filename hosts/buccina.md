# buccina

Overwrite this file when facts change; do not append a diary.

- **Hardware:** microserver. Intel e1000e Ethernet. NVMe disk.
- **Network:** DHCP, 10.0.0.149.
- **Overlay:** [buccina.json](buccina.json) — hostname and Mountain time.
- **Running now:** the 2026-09-30 Wicket trial image (root SSH with the
  Wicket deploy key, `@wicket-state` and `@appdata` subvolumes, kernel
  7.3.0-rc1 #4 after the ESP `vmlinuz` was replaced by hand). Wicket has
  left the core (R1); this image is not built from this tree any more.
- **Next:** wipe and reinstall as a plain base machine (roadmap phase 6).
  Wicket comes back later as its own packages.
