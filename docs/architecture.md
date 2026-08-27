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

Intended shape (not as-built — see
[ideas/2026-08-27-founding-brainstorm.md](ideas/2026-08-27-founding-brainstorm.md)
and open-questions): Linux kernel + **own PID 1**; live catalog under a
single tree; one `oath` admin surface; musl base; rollback via FS
snapshots.
