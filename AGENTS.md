# Oath — Contributor & Agent Guide

Oath is a **new Linux distribution**. Linux kernel, own userspace. Not a remix
of Debian, Arch, NixOS, Alpine, or Ubuntu. Principles: **Independence**,
**simplicity**, **openness**, **courage**.

The product is **AI-first**: the sysadmin is an agent. The running system must
teach a model that has never seen Oath how to find things and change them.
Unix is the substrate, not the admin interface.

## Session start

1. This file.
2. [`CURRENT.md`](CURRENT.md) — living priority and dogfood/runtime state.
3. [`docs/capabilities.md`](docs/capabilities.md) — as-built maturity for the
   slice you will touch.
4. [`docs/open-questions.md`](docs/open-questions.md) — any **Decision points**
   for the slice? If yes, **ask the human**; do not invent product policy.
5. Only the freeze or plan needed for the active domain.

If the user signals go-ahead without a new task ("go", "ok go", "continue",
etc.), execute **CURRENT.md → Now** — do not re-plan from scratch.

Update `CURRENT.md` and `docs/capabilities.md` when direction, capability
status, or known runtime state changes. **No** one-off handoff files.
[`.grok/rules/active-work.md`](.grok/rules/active-work.md) is only a
pointer to `CURRENT.md` (auto-load reminder).

**Skills (Grok):** `.grok/skills/` —

- `oath-session-start` — boot order above
- `oath-progress-docs` — **mandatory** end-of-slice doc updates

## Progress documentation is first-class (mandatory)

Describing the system and its progress is **paramount**. Incomplete meta work
means incomplete product work. Full model:
[`docs/progress-model.md`](docs/progress-model.md). Portable practice:
[`docs/progress-documentation-practice.md`](docs/progress-documentation-practice.md).

| Kind | Home | Role |
|------|------|------|
| Focus | Root `CURRENT.md` | Priority, next moves, dogfood facts, locks |
| As-built progress | `docs/capabilities.md` | Capability status + gaps |
| As-built map | `docs/architecture.md` | Processes, trees, paths, images |
| Target design | `docs/specs/*` | Freezes (desired shape) |
| Horizon | `docs/roadmap.md` | Phase-level program status |
| Product docs | `docs/manual/` | Current operator user manual |

**End of every real product slice (same change as code):**

1. Update capability row(s) (status and/or gaps).
2. Update `CURRENT.md` if priority or dogfood changed.
3. Update `docs/manual/` if operator-visible **shipped** behavior changed.
4. Update `architecture.md` if the system map changed.
5. Flip `roadmap.md` phase status only when phase-level status changes.
6. Follow [`.grok/skills/oath-progress-docs/SKILL.md`](.grok/skills/oath-progress-docs/SKILL.md).

Do not invent `STATUS.md` / `HANDOFF.md` / session diaries. Deferred
*improvements* to this meta system are listed only in
[`docs/progress-model.md`](docs/progress-model.md#deferred-meta-work).

**Code wins** over stale docs — then fix the docs immediately.

## What this is (and is not)

- **Is:** a from-scratch Linux distribution whose identity is a typed, live
  catalog an agent can read, plus one admin surface (`oath`) that is the same
  API humans and agents use. We **write PID 1**.
- **Is not:** a kernel project, a remix of another distro, a chatbot in
  PID 1, or systemd/dinit with a skin.
- **License:** MIT. [`LICENSE`](LICENSE). Copyright (c) Joshua Kifer.
  Upstream files keep their own licenses.
- **Libc:** musl base; glibc only as a runtime object. Never two libcs in
  one process.

Active freeze:
[`docs/specs/2026-08-27-catalog-and-oath-surface.md`](docs/specs/2026-08-27-catalog-and-oath-surface.md).
Active plan: **none** (Phase 1 and 2 complete).

Locks: root [`CURRENT.md`](CURRENT.md). Closed Decision points:
[`docs/open-questions.md`](docs/open-questions.md).

## Agent autonomy

**Do without asking:** read/search/list/edit project files; docs hygiene;
normal git (status, diff, log, add, commit, local branch); local QEMU of
images this repo built, once those exist.

**Still confirm or stop before:** force-push / hard reset of published
history; writing to a real disk / USB installer the user did not request;
changing the build machine’s OS as if it were Oath; committing secrets;
inventing answers to Decision points.

Never ask permission to read or search this repository.

## Tree

```
AGENTS.md             Contributor + agent guide
CURRENT.md            Living session focus (only handoff)
LICENSE               MIT (Joshua Kifer)
README.md             What this repo is
docs/
  README.md           Docs map + session boot
  progress-model.md   How progress docs work
  capabilities.md     As-built capability matrix
  architecture.md     As-built system map
  roadmap.md          Program horizon
  open-questions.md   Design forks + ask-human decisions
  specs/              Target freezes (dated)
  plans/              Implementation checklists
  ideas/              Parked thoughts
  manual/             Operator user manual (current behavior)
.grok/skills/         oath-session-start, oath-progress-docs
apocrypha/            gitignored scratch/history — not product
crates/oath-core      catalog, kinds, apply/undo
crates/oath           CLI (guest)
crates/oath-init      PID 1 + serial-login
crates/oath-make      host pack / QEMU / probe
image/                tools.nix only (borrowed prebuilts)
```

Do not invent extra kinds until CURRENT says so.

## Workflow

- Living progress: `CURRENT.md`. Specs/plans under `docs/` when a phase or
  slice starts.
- Work on `master` unless told otherwise. Small logical commits. Commit
  product + meta together.
- Never commit secrets. Never treat `apocrypha/` as source of truth.
- Do not mix “is” and “will be” in the same paragraph without a label.
- Do not assume Debian/Arch/NixOS conventions apply to the *product*.

## Grok tooling

- Project permissions: `.grok/config.toml` (no secrets there either), when
  it exists.
- Rules under `.grok/rules/` are pointers, not a second handoff.
