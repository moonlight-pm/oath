# Plan — devices + Unix floor

**Date:** 2026-08-30
**Status:** complete
**Proof:** probe `floor.*` + `dev.*` + `reboot.dev_vda` (2026-08-30).
**Freeze:** [../specs/2026-08-30-devices.md](../specs/2026-08-30-devices.md)

---

- [x] Kind `dev`: schema, seed vda/net0/ttyS0, refuse `present=false`.
- [x] Probe sysfs at boot (after net rename) and on apply/undo.
- [x] PID 1: tmpfs `/tmp` `/dev/shm` `/run`; cgroup2; virtio-rng;
      `/etc/hosts`.
- [x] Probe + tests; manual when probe passes.
