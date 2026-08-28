# Plan — Phase 2 svc closed loop

**Date:** 2026-08-28
**Status:** complete
**Freeze:** [../specs/2026-08-27-catalog-and-oath-surface.md](../specs/2026-08-27-catalog-and-oath-surface.md)
**Proof:** probe `hold.*` + `reboot.hold_stopped` (2026-08-28).

Do not add kinds. Do not disable `svc:serial` in the probe.

---

- [x] Init **converge**: start enabled, **stop** disabled, restart on
      exec change. Remove from the kid table **before** SIGTERM so reap
      does not restart a service we just stopped.
- [x] Seed `svc:hold` — `/bin/sleep 86400000`, enabled, `restart=always`.
- [x] `oath apply` on `svc` notifies PID 1 then **waits** for actual
      state (Live only; tests no-op).
- [x] `oath undo` notifies PID 1 after restoring the catalog.
- [x] Probe: hold running → disable → stopped → undo → running →
      disable → reboot → boot2 still stopped.
- [x] Manual [services.md](../manual/services.md) matches.
