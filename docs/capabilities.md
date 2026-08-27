# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: **shipped only**.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-08-27

Nothing is implemented. Rows below are the founding capability set so later
sessions have somewhere to hang status. All are **idea** until a freeze
exists.

---

## Founding band

| ID | Capability | Status | Spec / plan | Dogfood | Gaps | Manual |
|----|------------|--------|-------------|---------|------|--------|
| charter | Principles + independence + AI-first thesis | idea | [brainstorm](ideas/2026-08-27-founding-brainstorm.md) | — | D1–D9 closed. **Gaps:** no freeze yet | no |
| catalog | Live INDEX + schemas the agent reads first | idea | brainstorm | — | No freeze | no |
| objects | Typed system object store (desired + actual) | idea | brainstorm | — | No freeze | no |
| oath-cli | One admin surface (`oath`) = agent API | idea | brainstorm | — | No freeze | no |
| snap | Snapshot / rollback before mutate | idea | [snapshots note](ideas/2026-08-27-snapshots-and-libc-hybrid.md) | — | Principle: FS snapshots / generations. **Gaps:** no freeze; FS not picked (btrfs first candidate) | no |
| boot | Kernel + init + QEMU boot of an Oath image | idea | — | — | Own PID 1 is locked (D3). **Gaps:** no freeze, no image | no |
| pkg | Package objects (search, install, schema) | idea | — | — | No freeze | no |
| svc | Own supervisor; services are catalog objects | idea | D3 locked | — | Init will be ours. **Gaps:** no freeze, no code | no |
| net | Network objects | idea | — | — | No freeze | no |
| dev | Device objects | idea | — | — | No freeze | no |
| agent | Agent runtime as a system component | idea | — | — | No freeze | no |
| update | Base-image updates | idea | — | — | No freeze | no |
| install | Installer to a disk | idea | — | — | No freeze | no |
