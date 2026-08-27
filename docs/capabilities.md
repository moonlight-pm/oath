# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: **shipped only**.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-08-27

---

## Founding band

| ID | Capability | Status | Spec / plan | Dogfood | Gaps | Manual |
|----|------------|--------|-------------|---------|------|--------|
| charter | Principles + independence + AI-first thesis | spec’d | [CURRENT locks](../CURRENT.md) | — | none for v0 | no |
| catalog | Live INDEX + schemas | partial | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | QEMU probe | INDEX generated; `oath-tel` + `/oath/log` | [qemu](manual/qemu.md) limited |
| objects | Typed object store | partial | freeze | QEMU | host/svc/snap only | no |
| oath-cli | `oath` verbs | partial | freeze | QEMU probe | MCP later | limited |
| snap | generations; apply / undo | partial | freeze | probe (incl. reboot) | nested `/.oath-gens` on `@` | no |
| svc | Own supervisor | partial | freeze | `svc:serial` | notify socket barely used; no wants-order | no |
| boot | Kernel + init + QEMU | partial | [plan](plans/2026-08-27-qemu-skeleton-plan.md) | `oath-make run` / `probe` | borrowed kernel; sudo to pack disk | [qemu](manual/qemu.md) |
| pkg | Package objects | idea | — | — | not Phase 1 | no |
| net | Network objects | idea | — | — | serial instead | no |
| dev | Device objects | idea | — | — | not Phase 1 | no |
| agent | Agent runtime as a system component | idea | — | — | serial client is enough | no |
| update | Base-image updates | idea | — | — | generations are the primitive | no |
| install | Installer to a disk | idea | — | — | not Phase 1 | no |
