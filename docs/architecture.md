# Architecture

**Role:** as-built system map (what the code and runtime look like **now**).
**Not** the place for multi-feature roadmaps or session priority.

| Concern | Document |
|---------|----------|
| Capability maturity | [capabilities.md](capabilities.md) |
| Target design freezes | [specs/](specs/) |
| Session priority + dogfood | Root [CURRENT.md](../CURRENT.md) |
| How docs fit together | [progress-model.md](progress-model.md) |
| Product docs | [manual/](manual/) — **shipped only** |

---

## As-built (2026-08-27)

There is no OS tree yet. This repository contains documentation, agent
guide, and progress-docs skills.

When a QEMU image, `oath` binary, catalog tree, or supervisor exists, describe
them here: processes, paths, image layout, how to boot the dogfood.

**Target (not as-built):**
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md)
— `/oath` catalog, `kind:name` objects, `oath` verbs, own PID 1, musl,
btrfs generations.
