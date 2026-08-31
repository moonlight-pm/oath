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

**Status: partial**

Install/remove/query packages through the catalog. Services are objects
with schema, not unit-file folklore. Freeze:
[specs/2026-08-28-packages.md](specs/2026-08-28-packages.md). Canary
`pkg:hello` plus sealed `busybox` / `btrfs` / `oath` / `dropbear` are
in. `pkg:fetchme` wget canary is in. Hosting identity is T20 (`pkg.url`;
other Oath hosts as origin). Guest store export, hashes, deps, and
versions are not.

## Phase 4 — Devices and network

**Status: partial**

Hardware inventory as objects. One network model. The agent does not run
`lspci` / `ip` / `udevadm` and guess. Net canary:
[specs/2026-08-29-net.md](specs/2026-08-29-net.md),
[specs/2026-08-30-ssh-and-dhcp.md](specs/2026-08-30-ssh-and-dhcp.md)
(`net:net0` + SSH + `dev` inventory + virtio-gpu). DHCP, `OATH_BRIDGE`,
Unix floor, `svc` wants, and URL fetch exist. Module-as-catalog
and guest store export do not. Wayland is T21 River as `svc`; input
is T22 libinput via libudev-zero (no udevd).

## Phase 5 — Agent as a system component

**Status: planned**

A resident agent that boots into the catalog, with a protocol any model
can speak. Default client can be Grok; the OS interface is not Grok.

## Phase 6 — Disk install and updates

**Status: active**

Installer, A/B or equivalent base updates, honest rollback. First
slice is T27: replace canto (SSH+kexec). QEMU probe stays.

## Later (unplanned)

- Broad ABI compatibility for random Linux binaries
- Large binary repository
- Bare-metal diversity
- A graphical session (display canary + T21 River as `svc` + T22
  input + T23 session stack + T25 session manager + T26
  sola-terminal; other kit apps not)
