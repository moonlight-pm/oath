# Plan — Phase 3 base packages

**Date:** 2026-08-29
**Status:** complete
**Proof:** probe `pkg.*_symlink` + `pkg.*_refuse` + hello loop (2026-08-29).
**Freeze:** [../specs/2026-08-29-pkg-base.md](../specs/2026-08-29-pkg-base.md)

Do not add other packages. Do not fetch. Do not implement `wants`.

---

- [x] Seed `pkg:busybox`, `pkg:btrfs`, `pkg:oath` (`present=true`,
      `removable=false`). hello stays removable.
- [x] `set` / `apply` refuse `present=false` on non-removable pkgs.
- [x] Pack store trees; `/bin` is only a symlink farm (applets inside
      the busybox store). Pack links use prefix `/oath`, not the stage
      path.
- [x] Undo copies store **symlinks** as symlinks.
- [x] Probe: base ids, symlink targets, refuse absent; hello loop still
      passes.
- [x] Tests without QEMU (refuse absent).
- [x] Manual when the probe passes.
