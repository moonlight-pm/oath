# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-27

---

## Now

1. Phase 1: execute
   [docs/plans/2026-08-27-qemu-skeleton-plan.md](docs/plans/2026-08-27-qemu-skeleton-plan.md)
   against freeze
   [docs/specs/2026-08-27-catalog-and-oath-surface.md](docs/specs/2026-08-27-catalog-and-oath-surface.md).
2. Proof is the courage test in the freeze (hostname + reboot + undo
   on serial QEMU).
3. Do not grow kinds (pkg, dev, net, glibc runtime) in this slice.

**Explicit holds:** none on Phase 1 once work starts. Do not install to
a real disk. Do not treat qcow2 snapshots as the product undo.

**Always allowed:** docs hygiene; tests; progress-doc maintenance.

---

## Known dogfood state

| | **this repo** |
|--|----------------|
| Role | freeze + plan; no OS tree |
| Endpoint / host | none |
| Notes | nothing boots; no image; no `oath` binary |

---

## Locked models

Do not re-litigate without an explicit decision.

- **Name:** Oath. Linux kernel, own userspace. Not a remix. Principles:
  Independence, simplicity, openness, courage. AI-first: agent
  sysadmin, humans own policy, no chatbot in PID 1.
- **Init:** we write PID 1. `svc` objects are its only config.
- **License:** MIT. Copyright (c) Joshua Kifer. [`LICENSE`](LICENSE).
- **Catalog:** `/oath`. INDEX first. Identity `kind:name`. JSON
  documents + JSON Schema 2020-12. Desired ≠ actual files. `oath` is
  the only admin surface (text + `--json`; MCP later).
- **v0 kinds:** `host`, `svc`, `snap`. `set` writes desired; `apply`
  snapshots then converges; `undo` is last apply.
- **btrfs** generations on the guest. qcow2 snapshots are host debug.
- **Safety:** `mutate` vs `confirm` (`--confirm`). Halt / wipe /
  boot-generation (except undo last) are confirm. Agents do not pass
  `--confirm` unless the owner asked.
- **Seat:** root on **serial** is the owner. No second Unix user.
  Apply log records uid + tty.
- **Dogfood:** x86_64 QEMU, serial console, no desktop, no installer.
- **libc:** musl base; glibc only as a later runtime object. Never two
  libcs in one process.
- **Layout:** catalog is truth. No `/etc` hunting.
- **Bootstrap:** build tools are not the runtime. Borrowed prebuilts ok
  early.

---

## Pointers

- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Active freeze: [docs/specs/2026-08-27-catalog-and-oath-surface.md](docs/specs/2026-08-27-catalog-and-oath-surface.md)
- Active plan: [docs/plans/2026-08-27-qemu-skeleton-plan.md](docs/plans/2026-08-27-qemu-skeleton-plan.md)
- Open questions: [docs/open-questions.md](docs/open-questions.md)
- Roadmap: [docs/roadmap.md](docs/roadmap.md)
