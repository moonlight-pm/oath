# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-27

---

## Now

1. Write the first freeze: catalog + object model + `oath` surface.
   Services are objects; init is the apply engine for `svc`.
2. Then a QEMU-boot skeleton plan (Phase 1).
3. Do **not** start a kernel tree, package repo, or installer until that
   freeze exists.

**Explicit holds:** implementation of the OS itself.

**Always allowed:** docs hygiene; capturing ideas; tightening open-questions
and this file.

---

## Known dogfood state

| | **this repo** |
|--|----------------|
| Role | charter + progress docs; no OS tree |
| Endpoint / host | none |
| Notes | nothing boots; no image; no `oath` binary |

---

## Locked models

Do not re-litigate without an explicit decision.

- **Name:** Oath.
- **Kind:** a new Linux distribution (Linux kernel, own userspace). Not a
  kernel project.
- **Independence:** not a remix of Debian, Arch, NixOS, Alpine, or Ubuntu.
  Ideas may be borrowed; identity, package story, init, and admin surface
  are ours.
- **Principles:** Independence, simplicity, openness, courage.
- **AI-first:** the sysadmin is an agent. The live system must be
  discoverable to a model with no Oath training data. Humans own policy;
  agents operate. Not a chatbot in PID 1.
- **Progress docs:** the portable practice in
  [`docs/progress-documentation-practice.md`](docs/progress-documentation-practice.md).
  One living handoff (`CURRENT.md`).
- **Init:** we write and own PID 1 + supervisor (Rust). No systemd, no
  dinit, no throwaway foreign init. Time is not a reason to wrap
  someone else’s. Service config **is** the catalog — no unit dialect.
- **License:** MIT. Copyright (c) Joshua Kifer. See [`LICENSE`](LICENSE).
  Upstream files keep their own licenses.
- **Dogfood shape:** QEMU headless appliance first. No desktop, no
  bare-metal installer, in Phase 1. Success is a VM whose catalog an
  agent can read.
- **Rollback:** filesystem snapshots as the undo primitive (`oath apply`
  / generations). Filesystem not locked; btrfs subvolumes are the first
  candidate. qcow2 snapshots are host debug only.
- **libc:** musl is the base. glibc may exist as a **runtime object** for
  payloads that only exist as glibc binaries. Never two libcs in one
  process. No year-one ABI promise for random foreign binaries.
- **Packages:** catalog objects, not a language. No foreign archive as
  identity. Agents never `apt upgrade` the world.
- **Layout:** the catalog tree is truth. Agents do not edit `/etc` as
  policy. A compatibility shard (`/bin`, `/usr`, …) exists only to
  execute what we shipped.
- **Desired state:** typed documents under the catalog tree, matching a
  schema, with an apply log that records the filesystem generation.
  Undo is the snapshot, not a second store.
- **Agent protocol:** catalog and `oath` are agent-agnostic. A model may
  be the first *client*; it is not the OS interface. No vendor API in
  the object schema.
- **Bootstrap:** tools used to *produce* images are not the runtime.
  The artifact is an Oath image. Borrowed prebuilts may appear early;
  replace inward over time.

---

## Pointers

- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Open questions: [docs/open-questions.md](docs/open-questions.md)
- Founding brainstorm: [docs/ideas/2026-08-27-founding-brainstorm.md](docs/ideas/2026-08-27-founding-brainstorm.md)
- Snapshots / musl hybrid: [docs/ideas/2026-08-27-snapshots-and-libc-hybrid.md](docs/ideas/2026-08-27-snapshots-and-libc-hybrid.md)
- Roadmap: [docs/roadmap.md](docs/roadmap.md)
- Active plan: none
- Active freeze: none
