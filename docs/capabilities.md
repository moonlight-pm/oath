# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: **shipped only**.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-08-27

Nothing is implemented. Freeze exists for the catalog surface.

---

## Founding band

| ID | Capability | Status | Spec / plan | Dogfood | Gaps | Manual |
|----|------------|--------|-------------|---------|------|--------|
| charter | Principles + independence + AI-first thesis | spec’d | [CURRENT locks](../CURRENT.md) | — | none for v0 | no |
| catalog | Live INDEX + schemas the agent reads first | spec’d | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | — | no code | no |
| objects | Typed system object store (desired + actual) | spec’d | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | — | no code | no |
| oath-cli | One admin surface (`oath`) = agent API | spec’d | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | — | no code | no |
| snap | btrfs generations; apply / undo | spec’d | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | — | no code | no |
| svc | Own supervisor; services are catalog objects | spec’d | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | — | no code | no |
| boot | Kernel + init + QEMU boot | planned | [plan](plans/2026-08-27-qemu-skeleton-plan.md) | — | no image | no |
| pkg | Package objects | idea | — | — | not Phase 1 | no |
| net | Network objects | idea | — | — | not Phase 1; serial instead | no |
| dev | Device objects | idea | — | — | not Phase 1 | no |
| agent | Agent runtime as a system component | idea | — | — | serial client is enough | no |
| update | Base-image updates | idea | — | — | generations are the primitive | no |
| install | Installer to a disk | idea | — | — | not Phase 1 | no |
