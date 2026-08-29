# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-29

---

## Now

1. **Phase 3 canary shipped** (`pkg:hello`). Do not wrap busybox or
   add more packages unless asked.
2. Do not add net / dev / glibc runtime / installer unless CURRENT is
   updated.
3. Do not install to a real disk.

**Always allowed:** docs hygiene; tests; `cargo make build|run|probe`.

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Phase 1+2 serial box |
| How | `cargo make build` then `probe` / `run` |
| Notes | Hostname + `svc:hold` + `pkg:hello` install/remove/undo/reboot persist. Manual: `docs/manual/`. Telemetry: `build/runs/<id>/`. |

```sh
nix-shell
cargo make build
cargo make probe
cargo make run
```

---

## Locked models

Do not re-litigate without an explicit decision.

- Oath: Linux kernel, own userspace, musl base, own PID 1.
- Catalog `/oath`, ids `kind:name`, `oath` is the only admin surface.
- v0 kinds `host`, `svc`, `snap`. Phase 3 adds `pkg` (canary
  `pkg:hello` only). Sibling `@gen-N` at `/oath/run/fs`.
- Packages: store `/oath/store/pkg/<name>/`; `/bin` is a symlink farm;
  no new verbs; no fetch in v0.
- Services: PID 1 converges enabled/disabled `svc:*`. `svc:serial` is
  the console; `svc:hold` is the start/stop test process.
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-28-packages.md](docs/specs/2026-08-28-packages.md)
  (catalog:
  [docs/specs/2026-08-27-catalog-and-oath-surface.md](docs/specs/2026-08-27-catalog-and-oath-surface.md))
- Plan: [docs/plans/2026-08-28-pkg-canary-plan.md](docs/plans/2026-08-28-pkg-canary-plan.md)
  (complete)
- Roadmap: Phase 3 packages (active; canary done)
