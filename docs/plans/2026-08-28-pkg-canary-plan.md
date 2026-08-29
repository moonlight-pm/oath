# Plan — Phase 3 pkg canary

**Date:** 2026-08-28
**Status:** complete
**Proof:** probe `pkg.*` + `reboot.pkg_hello` (2026-08-29).
**Freeze:** [../specs/2026-08-28-packages.md](../specs/2026-08-28-packages.md)

At most one open plan. Checkboxes are implementation work.

Do not wrap busybox. Do not add kinds besides `pkg`. Do not fetch.

---

- [x] Kind `pkg`: schema JSON + Markdown; INDEX one-liner; seed
      `pkg:hello` `present=false`.
- [x] Pack `/oath/store/pkg/hello/bin/hello` (prints `hello`).
- [x] `oath apply` converges `pkg`: symlink farm in `/bin`; record
      `actual.links`; refuse to clobber a name this object does not own.
- [x] Undo restores `store/` with the catalog, then converges `pkg:*`.
- [x] Probe: absent → present (symlink + run) → reboot persist →
      absent → undo restores.
- [x] Tests on the catalog (present/absent, collision) without QEMU.
- [x] Manual page when the probe passes (not before).
