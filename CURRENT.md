# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-27

---

## Now

1. Phase 1 remainder: quieter init, sibling subvols for generations
   (not nested under `/`). Courage test including **reboot** passed.
2. Do not grow kinds (pkg, dev, net, glibc runtime).
3. Do not install to a real disk. qcow2 snapshots are not product undo.

**Always allowed:** docs hygiene; tests; `image/build.sh` / `image/run.sh`.

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Phase 1 serial box |
| How | `nix-shell` (optional) → `./image/build.sh` → `./image/run.sh` |
| Notes | Courage test passed 2026-08-27 (hostname apply, undo, confirm-reboot, hostname **survives reboot**). Telemetry: `build/runs/<id>/`. Loop-pack uses `sudo`. |

```sh
nix-shell
./image/build.sh
./image/probe.py    # scripted courage test + run dir
./image/run.sh      # interactive serial; also writes build/runs/
```

---

## Locked models

Do not re-litigate without an explicit decision. See previous locks plus
the catalog freeze. Short form:

- Oath: Linux kernel, own userspace, musl base, own PID 1.
- Catalog `/oath`, ids `kind:name`, `oath` is the only admin surface.
- v0 kinds `host`, `svc`, `snap`. btrfs generations. Serial QEMU x86_64.
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Active freeze: [docs/specs/2026-08-27-catalog-and-oath-surface.md](docs/specs/2026-08-27-catalog-and-oath-surface.md)
- Active plan: [docs/plans/2026-08-27-qemu-skeleton-plan.md](docs/plans/2026-08-27-qemu-skeleton-plan.md)
- QEMU (limited): [docs/manual/qemu.md](docs/manual/qemu.md)
