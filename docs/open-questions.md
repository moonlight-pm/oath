# Open questions

Unresolved **design forks**. Not the implementation backlog
(that lives in roadmap / capabilities / plans).

Agents: if work depends on a Decision point, **stop and ask**. Record the
answer in the decision log and update CURRENT locks.

Founding D1–D9 and the catalog freeze’s technical locks are **closed**.

---

## Decision points (ask human)

None open at founding or catalog-freeze scope.

---

## Open technical questions

- Bootloader hook to boot `@gen-N` (layout is `@` + `@gen-N` at
  `/oath/run/fs`; picker not built).
- PID 1 notify socket bytes (`/oath/run/…`) — implementation detail;
  must not become a second config format.
- How much of coreutils we replace vs busybox vs our own (Phase 1 may
  borrow busybox).
- glibc runtime object layout — not Phase 1.

---

## Closed

### D1 — First dogfood shape (P0) — locked 2026-08-27

QEMU headless appliance. No desktop. No bare-metal installer in Phase 1.
Success is a VM whose catalog an agent can read. **x86_64**, **serial
console**.

### D2 — libc and foreign ABI (P0) — locked 2026-08-27

**musl base.** glibc may exist later as a catalog runtime object. Never
mix libcs in one process.

### D3 — Init / supervisor (P0) — locked 2026-08-27

Own PID 1 + supervisor in Rust. `svc` objects are the only service
config.

### D4 — Package and update model (P0) — locked 2026-08-27

Packages as catalog objects (later). Rollback via **btrfs** subvolume
generations. qcow2 snapshots are host debug only.

### D5 — Filesystem layout (P0) — locked 2026-08-27

Catalog tree is **`/oath`**. Agents do not edit `/etc`. Compatibility
shard only to execute what we shipped.

### D6 — Desired-state store (P1) — locked 2026-08-27

Pretty-printed JSON under `/oath/objects/<kind>/<name>/{desired,actual,meta}.json`.
Apply log JSONL. Undo is the filesystem snapshot.

### D7 — Agent coupling (P1) — locked 2026-08-27

`oath` text + `--json`. MCP later. No vendor API in the schema.

### D8 — Bootstrap / build host (P1) — locked 2026-08-27

Build tools are not the runtime. Borrowed prebuilts ok in Phase 1.

### D9 — License (P2) — locked 2026-08-27

MIT. Copyright (c) Joshua Kifer. [`LICENSE`](../LICENSE).

### T1 — Catalog path — locked 2026-08-27

`/oath`. INDEX at `/oath/INDEX.md`.

### T2 — Object identity — locked 2026-08-27

`kind:name`. No UUIDs in v0.

### T3 — Schema language — locked 2026-08-27

JSON Schema 2020-12 until it hurts. Kind prose in Markdown.

### T4 — `oath` views — locked 2026-08-27

One verb set. Text default, `--json` same facts. MCP later.

### T5 — v0 kinds — locked 2026-08-27

`host`, `svc`, `snap`. No net/pkg/dev in Phase 1. Serial is how the
agent reaches the VM.

### T6 — Snapshot filesystem — locked 2026-08-27

btrfs for Phase 1.

### T7 — Agent reachability — locked 2026-08-27

QEMU serial. SSH later.

### T8 — Architecture — locked 2026-08-27

`x86_64` only in Phase 1.

### T9 — Principal — locked 2026-08-27

Root on serial is the owner. Agent is not a second Unix user. Log
uid + tty.

### T10 — Confirm class — locked 2026-08-27

`mutate` vs `confirm`. Halt, wipe, boot-generation (except undo last)
need `--confirm`. Agents do not pass it unless the owner asked.

---

## Decision log

| Date | ID | Decision | Where recorded |
|------|-----|----------|----------------|
| 2026-08-27 | D1–D9 | Founding locks | this file Closed; [CURRENT.md](../CURRENT.md) |
| 2026-08-27 | T1–T10 | Catalog freeze technical locks | this file; [specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md) |
