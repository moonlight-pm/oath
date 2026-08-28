# Plan — Phase 3 pkg canary

**Date:** 2026-08-28
**Status:** open
**Freeze:** [../specs/2026-08-28-packages.md](../specs/2026-08-28-packages.md)

At most one open plan. Checkboxes are implementation work.

Do not wrap busybox. Do not add kinds besides `pkg`. Do not fetch.

---

- [ ] Kind `pkg`: schema JSON + Markdown; INDEX one-liner; seed
      `pkg:hello` `present=false`.
- [ ] Pack `/oath/store/pkg/hello/bin/hello` (prints `hello`).
- [ ] `oath apply` converges `pkg`: symlink farm in `/bin`; record
      `actual.links`; refuse to clobber a name this object does not own.
- [ ] Undo restores `store/` with the catalog, then converges `pkg:*`.
- [ ] Probe: absent → present (symlink + run) → reboot persist →
      absent → undo restores.
- [ ] Tests on the catalog (present/absent, collision) without QEMU.
- [ ] Manual page when the probe passes (not before).
