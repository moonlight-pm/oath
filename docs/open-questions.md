# Open questions

Unresolved **design forks**. Not the implementation backlog
(that lives in roadmap / capabilities / plans).

Agents: if work depends on a Decision point, **stop and ask**. Record the
answer in the decision log and update CURRENT locks.

Founding D1–D9 are **closed**. New forks that would change the first
freeze belong here.

---

## Decision points (ask human)

None open at founding scope. Remaining questions are technical (below)
and can be answered in a freeze unless they collide with a closed lock.

---

## Open technical questions

These are the ones worth settling in (or beside) the first freeze.

- Exact name of the live catalog tree (`/oath` vs `/sys/oath` vs other).
- Object identity: `kind` + name? a stable id? both?
- Schema language (JSON Schema, something smaller).
- Whether `oath` speaks a text CLI, a JSON API, an MCP server, or all
  three as views of one surface.
- v0 object kinds (host, svc, generation, net — what is in the courage
  test, what waits).
- Snapshot filesystem: btrfs vs bcachefs vs other (principle is locked
  to FS snapshots; implementation is not).
- How a glibc runtime object is laid out (sysroot, loader path, how
  `oath` execs into it) — not Phase 1.
- How the agent reaches the QEMU appliance (serial, ssh, both).
- Target architecture for Phase 1 (`x86_64` only?).
- Who the principal is on the appliance (root-only seat, a named user,
  the agent as a system identity).
- Which applies are automatic vs which require the owner.
- How much of coreutils we replace vs busybox vs our own.

---

## Closed

### D1 — First dogfood shape (P0) — locked 2026-08-27

QEMU headless appliance. No desktop. No bare-metal installer in Phase 1.
Success is a VM whose catalog an agent can read.

### D2 — libc and foreign ABI (P0) — locked 2026-08-27

**musl base.** Almost no foreign ABI promise for random binaries. glibc
may exist as a catalog **runtime** for shipped payloads that only exist
as glibc. Never mix libcs in one process. Do not rebuild Chromium-scale
stacks until the base exists.

### D3 — Init / supervisor (P0) — locked 2026-08-27

We **write and own** PID 1 + supervisor in Rust. No systemd. No dinit.
No throwaway init. Time is not a reason to wrap someone else’s. Service
configuration **is** the catalog — init has no unit-file dialect.

### D4 — Package and update model (P0) — locked 2026-08-27

Packages as catalog objects (not a language). No foreign archive as
identity. Agents never `apt upgrade` the world. **Rollback uses
filesystem snapshots** (generations): `oath apply` snapshots, mutates,
records the generation; undo rolls desired and actual together.
Boot-time rollback can be “pick a generation.” Filesystem not locked;
**btrfs subvolumes are the first candidate.** qcow2 snapshots are host
debug only.

### D5 — Filesystem layout (P0) — locked 2026-08-27

Catalog tree is truth. Agents are forbidden from editing `/etc` as
policy. Keep a compatibility shard (`/bin`, `/usr`, …) only as needed to
execute what we shipped. Do not make FHS the identity.

### D6 — Desired-state store (P1) — locked 2026-08-27

A directory of typed documents under the catalog tree, each matching a
schema, with an apply log that records the filesystem generation. Avoid
a secret binary database the agent cannot `cat`. Undo is the FS
snapshot, not a second store.

### D7 — Agent coupling (P1) — locked 2026-08-27

Protocol and catalog are agent-agnostic. A model may be the first
*client*, not the interface. No vendor API in the object schema.

### D8 — Bootstrap / build host (P1) — locked 2026-08-27

Tools used to produce images are not the installed OS. The artifact is
an Oath image. Replace borrowed prebuilts inward over time; do not wait
for a full from-source world before Phase 1.

### D9 — License (P2) — locked 2026-08-27

MIT. Copyright (c) Joshua Kifer. Text in [`LICENSE`](../LICENSE).
Upstream files keep their licenses. Do not invent a custom license.

---

## Decision log

| Date | ID | Decision | Where recorded |
|------|-----|----------|----------------|
| 2026-08-27 | — | Name, kind, principles, AI-first, not-a-remix, progress-docs practice | [CURRENT.md](../CURRENT.md) locked models |
| 2026-08-27 | D1 | QEMU headless appliance first | CURRENT; this file Closed |
| 2026-08-27 | D2 | musl base; glibc only as a runtime object | CURRENT; this file Closed |
| 2026-08-27 | D3 | Own PID 1 + supervisor in Rust. Catalog is the service config. Time is not a factor. | CURRENT; this file Closed |
| 2026-08-27 | D4 | Packages as objects; rollback via FS snapshot generations; btrfs first candidate | CURRENT; this file Closed |
| 2026-08-27 | D5 | Catalog tree is truth; no `/etc` hunting | CURRENT; this file Closed |
| 2026-08-27 | D6 | Typed documents + apply log + FS generation | CURRENT; this file Closed |
| 2026-08-27 | D7 | Agent-agnostic protocol; model is a client | CURRENT; this file Closed |
| 2026-08-27 | D8 | Build tools are not the runtime | CURRENT; this file Closed |
| 2026-08-27 | D9 | MIT, Copyright (c) Joshua Kifer | [`LICENSE`](../LICENSE); CURRENT |
