# Roadmap

**Program horizon** — coarse phase status over months.
Day-to-day maturity: [capabilities.md](capabilities.md).
Session priority: root [CURRENT.md](../CURRENT.md).

| Status | Meaning |
|--------|---------|
| **done** | Good enough to build on; polish ok |
| **partial** | Scaffold or subset shipped; important gaps remain |
| **active** | Current focus (`CURRENT.md`) |
| **next** | Queued after active |
| **planned** | Intended; not started |
| **unplanned** | Not scheduled; capture only |

Update phase status only when a **phase-level** flip happens.
Prefer capability rows for feature-level progress.

---

## Phase 0 — Charter

**Status: done**

Founding D1–D9 and catalog technical locks T1–T10 are closed. Freeze:
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md).

## Phase 1 — Skeleton boot

**Status: done**

QEMU boots our PID 1 and catalog. Courage test (hostname apply, undo,
reboot) passes. Generations are sibling `@gen-N` subvolumes. Plan:
[plans/2026-08-27-qemu-skeleton-plan.md](plans/2026-08-27-qemu-skeleton-plan.md).

## Phase 2 — Closed admin loop

**Status: done**

Hostname and `svc:hold` start/stop through the catalog, with undo and
reboot persist. Plan:
[plans/2026-08-28-svc-loop-plan.md](plans/2026-08-28-svc-loop-plan.md).

## Phase 3 — Packages and services as objects

**Status: next**

Install/remove/query packages through the catalog. Services are objects
with schema, not unit-file folklore.

## Phase 4 — Devices and network

**Status: planned**

Hardware inventory as objects. One network model. The agent does not run
`lspci` / `ip` / `udevadm` and guess.

## Phase 5 — Agent as a system component

**Status: planned**

A resident agent that boots into the catalog, with a protocol any model
can speak. Default client can be Grok; the OS interface is not Grok.

## Phase 6 — Disk install and updates

**Status: planned**

Installer, A/B or equivalent base updates, honest rollback.

## Later (unplanned)

- Broad ABI compatibility for random Linux binaries
- Large binary repository
- Bare-metal diversity
- A graphical session
