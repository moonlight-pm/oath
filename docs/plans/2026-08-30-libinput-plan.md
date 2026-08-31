# Plan — Libinput without udev

**Date:** 2026-08-30
**Status:** complete
**Proof:** probe `river.keyboard` / `river.pointer` (2026-08-31).
**Freeze:** [../specs/2026-08-30-libinput.md](../specs/2026-08-30-libinput.md)

- [x] T22 freeze: path backend, no udevd, `dev:kbd0` / `dev:mouse0`.
- [x] `forks/wlroots`: udev finds nothing → libinput path on
      `/dev/input/event*`. Log device add at info.
- [x] Initramfs + PID 1 load `evdev`. Drop `WLR_LIBINPUT_NO_DEVICES`.
      Pack libudev-zero as `libudev.so.1`; pack libinput quirks.
- [x] Seed + probe `dev:kbd0` / `dev:mouse0` from sysfs names.
- [x] Probe: event node, catalog, river.log. Tests. Manual.
