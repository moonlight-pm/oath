# Plan — display canary

**Date:** 2026-08-30
**Status:** complete
**Proof:** probe `drm.card0` / `dev.card0` (2026-08-30).
**Freeze:** [../specs/2026-08-30-display.md](../specs/2026-08-30-display.md)

- [x] QEMU virtio-gpu + gtk when DISPLAY; probe stays `-display none`.
- [x] Initramfs modules: virtio_dma_buf, virtio-gpu, virtio_input.
- [x] `dev:card0`; kernel `console=tty0`; banner on `/dev/tty0`.
- [x] Probe `/dev/dri/card0`; tests; manual.
