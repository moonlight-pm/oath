**Date:** 2026-08-30
**Status:** target (freeze)
**Implementation:** partial
**Dogfood:** QEMU probe — `svc` wants order + `pkg:fetchme` wget
**Gaps:** no package index/repo; fetch is a URL on the object
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Service wants and package fetch

Closes leftover text-OS gaps on `svc` and `pkg`. Same verbs. No
graphical session.

---

## Locks

- `svc.wants` is **start order**. Wanted services that are enabled
  start first. Unknown/disabled wants are ignored (not Requires).
  **Cycles refuse apply** and PID 1 will not start the cyclic set.
- `pkg.url` optional. If `present` and the store file is missing,
  apply `wget`s the URL into `/oath/store/pkg/<name>/bin/<name>` then
  links `/bin`. Local store wins if already there. No new verbs.
- v0 fetch canary: **`pkg:fetchme`**, URL
  `http://10.0.2.2:18765/fetchme` (QEMU slirp host). Removable.
- `svc:hold` wants `svc:serial`.

---

## Out

- Package repository / deps / versions
- Resident agent (Phase 5)
- Disk installer (Phase 6)
- Graphics / Sola
