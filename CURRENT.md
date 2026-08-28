# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-28

---

## Now

1. Phase 2: closed **svc** loop — `oath set` / `apply` starts or stops a
   service, it survives reboot, undo works. Notify socket is the
   converge path. No new kinds.
2. Do not add pkg / net / dev / glibc runtime / installer.
3. Do not install to a real disk. qcow2 snapshots are not product undo.

**Always allowed:** docs hygiene; tests; `cargo run -p oath-make -- build|run|probe`.

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Phase 1 serial box |
| How | `nix-shell` (optional) → `cargo run -p oath-make -- build` → `probe` / `run` |
| Notes | Courage test green 2026-08-28 including sibling gens (`/oath/run/fs/@gen-N`) and reboot. Telemetry in `build/runs/<id>/`. |

```sh
nix-shell
cargo run -p oath-make -- build
cargo run -p oath-make -- probe
cargo run -p oath-make -- run
```

---

## Locked models

Do not re-litigate without an explicit decision. See previous locks plus
the catalog freeze. Short form:

- Oath: Linux kernel, own userspace, musl base, own PID 1.
- Catalog `/oath`, ids `kind:name`, `oath` is the only admin surface.
- v0 kinds `host`, `svc`, `snap`. btrfs generations. Serial QEMU x86_64.
- Generations are sibling subvolumes `@gen-N` beside live `@`, viewed
  at `/oath/run/fs` (btrfs top-level).
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Active freeze: [docs/specs/2026-08-27-catalog-and-oath-surface.md](docs/specs/2026-08-27-catalog-and-oath-surface.md)
- Active plan: none (Phase 1 plan complete). Next: Phase 2 svc loop.
- QEMU (limited): [docs/manual/qemu.md](docs/manual/qemu.md)
