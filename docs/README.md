# Oath documentation

Canonical **engineering** documentation for the Oath Linux distribution.

**Operator-facing product docs** live in [`manual/`](manual/) — current
behavior only, written as a user manual. See [`progress-model.md`](progress-model.md).

## Session boot (agents and humans)

| Step | Document |
|-----:|----------|
| 1 | [`../AGENTS.md`](../AGENTS.md) — autonomy, what this is, tree |
| 2 | [`../CURRENT.md`](../CURRENT.md) — **living** priority + dogfood state |
| 3 | [`capabilities.md`](capabilities.md) — as-built maturity for the slice |
| 4 | [`open-questions.md`](open-questions.md) Decision points if the slice needs policy |
| 5 | One freeze under [`specs/`](specs/) or plan under [`plans/`](plans/) if needed |

When you finish real product work, update **`CURRENT.md`** and
**`capabilities.md`** (and manual / architecture / roadmap as required) in the
**same change**. Follow
[`.grok/skills/oath-progress-docs/SKILL.md`](../.grok/skills/oath-progress-docs/SKILL.md).
Do not create one-off handoff files. Do not invent answers to Decision points —
ask the human.

**Progress docs are first-class.** Full rules:
[`progress-model.md`](progress-model.md) · portable export:
[`progress-documentation-practice.md`](progress-documentation-practice.md).

## Document map

| File | Purpose | Kind |
|------|---------|------|
| [`../CURRENT.md`](../CURRENT.md) | Living priority, dogfood, locks | **Focus** |
| [`capabilities.md`](capabilities.md) | Capability status + gaps | **As-built** |
| [`architecture.md`](architecture.md) | Processes, trees, image layout | **As-built** map |
| [`progress-model.md`](progress-model.md) | How is / will-be / focus / manual fit | Meta |
| [`progress-documentation-practice.md`](progress-documentation-practice.md) | Portable practice (shareable) | Meta |
| [`roadmap.md`](roadmap.md) | Coarse multi-month phases | **Horizon** |
| [`open-questions.md`](open-questions.md) | Design forks + ask-human decisions | Design forks |
| [`specs/`](specs/) | Target freezes (dated) | **Target** |
| [`specs/2026-08-27-catalog-and-oath-surface.md`](specs/2026-08-27-catalog-and-oath-surface.md) | Catalog, objects, `oath` | **Target** |
| [`specs/2026-08-28-packages.md`](specs/2026-08-28-packages.md) | Kind `pkg`, store, canary | **Target** (active) |
| [`plans/`](plans/) | Implementation checklists | Build |
| [`plans/2026-08-27-qemu-skeleton-plan.md`](plans/2026-08-27-qemu-skeleton-plan.md) | Phase 1 QEMU skeleton | **Build** (complete) |
| [`plans/2026-08-28-svc-loop-plan.md`](plans/2026-08-28-svc-loop-plan.md) | Phase 2 svc loop | **Build** (complete) |
| [`plans/2026-08-28-pkg-canary-plan.md`](plans/2026-08-28-pkg-canary-plan.md) | Phase 3 `pkg:hello` | **Build** (complete) |
| [`ideas/`](ideas/) | Parked thoughts | Idea |
| [`manual/`](manual/) | Operator user manual | **Product** (current only) |

## Related trees (not under `docs/`)

| Path | Role |
|------|------|
| `AGENTS.md` | Contributor + agent guide |
| `CURRENT.md` | Only living session handoff |
| `LICENSE` | MIT — Copyright (c) Joshua Kifer |
| `crates/` | `oath-core`, `oath`, `oath-init`, `oath-make` |
| `image/` | `tools.nix` borrowed prebuilts |
| `apocrypha/` | Scratch / history — not built, gitignored |
| `.grok/skills/` | `oath-session-start`, `oath-progress-docs` |
| `.grok/rules/active-work.md` | **Pointer** to `CURRENT.md` |

## Authority order

1. **Code that ships** (and tests)
2. **Root `CURRENT.md`** for active priority and dogfood facts
3. **`docs/capabilities.md`** for capability maturity
4. This `docs/` suite for intent and map
5. `apocrypha/` — ignore unless hunting history
