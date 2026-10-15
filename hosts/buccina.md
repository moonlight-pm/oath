# buccina

Overwrite this file when facts change; do not append a diary.

- **Hardware:** microserver. Intel e1000e Ethernet. NVMe disk.
- **Network:** DHCP, 10.0.0.149 (MAC `08:92:04:e3:29:15`).
- **Overlay:** [buccina.json](buccina.json) — hostname and Mountain time.
- **Running now:** the 2026-09-30 Wicket trial image (root SSH with the
  Wicket deploy key, `@wicket-state` and `@appdata` subvolumes, kernel
  7.3.0-rc1 #4 after the ESP `vmlinuz` was replaced by hand). Wicket has
  left the core (R1); this image is not built from this tree any more.
  On 2026-10-14 it answered with dropbear 2025.89 and an ED25519 host
  key that differs from the one novus recorded, and root refused the
  build host's `id_rsa`. Not entered since.
- **Next:** wipe and reinstall as a plain base machine (roadmap phase 6),
  either with the Wicket deploy key as `--target root@10.0.0.149` or
  from a `cargo make install --usb` stick. Wicket comes back later as
  its own packages.
