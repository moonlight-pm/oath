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

**Status: active**

A QEMU image that boots *our* init and presents the catalog, even if most
of userspace is still a small borrowed set. Plan:
[plans/2026-08-27-qemu-skeleton-plan.md](plans/2026-08-27-qemu-skeleton-plan.md).
Proof: courage test in the freeze.

## Phase 2 — Closed admin loop

**Status: planned**

Agent (or human using the same CLI) can discover an object, change it,
apply, and see it survive reboot. First objects: hostname, a service, a
network address. Snapshots around mutate.

## Phase 3 — Packages and services as objects

**Status: planned**

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
