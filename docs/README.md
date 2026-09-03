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
| [`specs/2026-08-28-packages.md`](specs/2026-08-28-packages.md) | Kind `pkg`, store, canary | **Target** |
| [`specs/2026-08-29-pkg-base.md`](specs/2026-08-29-pkg-base.md) | Sealed busybox / btrfs / oath | **Target** |
| [`specs/2026-08-29-net.md`](specs/2026-08-29-net.md) | `net:net0` static | **Target** |
| [`specs/2026-08-30-ssh-and-dhcp.md`](specs/2026-08-30-ssh-and-dhcp.md) | SSH keys + dhcp | **Target** |
| [`specs/2026-08-30-devices.md`](specs/2026-08-30-devices.md) | Device inventory + Unix floor | **Target** |
| [`specs/2026-08-30-wants-and-fetch.md`](specs/2026-08-30-wants-and-fetch.md) | wants + fetch | **Target** |
| [`specs/2026-08-30-display.md`](specs/2026-08-30-display.md) | virtio-gpu display canary | **Target** (shipped) |
| [`specs/2026-08-30-pkg-hosting.md`](specs/2026-08-30-pkg-hosting.md) | T20 hosting (`pkg.url`, peer origin) | **Target** (not implemented) |
| [`specs/2026-08-30-sola.md`](specs/2026-08-30-sola.md) | T21 Sola on Oath, River first | **Target** (River/seatd in) |
| [`specs/2026-08-30-libinput.md`](specs/2026-08-30-libinput.md) | T22 libinput without udev | **Target** (shipped) |
| [`specs/2026-08-30-oath-sola.md`](specs/2026-08-30-oath-sola.md) | T23 Sola session on Oath | **Target** (session stack) |
| [`specs/2026-08-31-sola-dev.md`](specs/2026-08-31-sola-dev.md) | T24 Sola/app development layout | **Target** (identity; Oath-as-dev-host started) |
| [`specs/2026-08-31-sola-session.md`](specs/2026-08-31-sola-session.md) | T25 sola-session as `svc` | **Target** (session manager) |
| [`specs/2026-08-31-sola-terminal.md`](specs/2026-08-31-sola-terminal.md) | T26 sola-terminal | **Target** (first kit app) |
| [`specs/2026-08-31-metal-canto.md`](specs/2026-08-31-metal-canto.md) | T27 canto metal canary | **Target** (partial; EFI/USB) |
| [`specs/2026-09-01-sola-browser.md`](specs/2026-09-01-sola-browser.md) | T28 sola-browser + CEF | **Target** (shipped; canto) |
| [`specs/2026-09-02-sola-workspaces.md`](specs/2026-09-02-sola-workspaces.md) | T29 sola-workspaces + solactl | **Target** (shipped; canto; no git/grok) |
| [`specs/2026-09-02-pkg-grok.md`](specs/2026-09-02-pkg-grok.md) | T30 `pkg:grok` (vendor-updating pkgs) | **Target** (identity; ELF not packed) |
| [`specs/2026-09-02-seat-home.md`](specs/2026-09-02-seat-home.md) | T31 seat `home` + `/lib/oath` + env | **Target** (canto `ssh home@canto`; graphical svcs off) |
| [`plans/`](plans/) | Implementation checklists | Build |
| [`plans/2026-08-27-qemu-skeleton-plan.md`](plans/2026-08-27-qemu-skeleton-plan.md) | Phase 1 QEMU skeleton | **Build** (complete) |
| [`plans/2026-08-28-svc-loop-plan.md`](plans/2026-08-28-svc-loop-plan.md) | Phase 2 svc loop | **Build** (complete) |
| [`plans/2026-08-28-pkg-canary-plan.md`](plans/2026-08-28-pkg-canary-plan.md) | Phase 3 `pkg:hello` | **Build** (complete) |
| [`plans/2026-08-29-pkg-base-plan.md`](plans/2026-08-29-pkg-base-plan.md) | Phase 3 base packages | **Build** (complete) |
| [`plans/2026-08-29-net-canary-plan.md`](plans/2026-08-29-net-canary-plan.md) | Phase 4 `net:net0` | **Build** (complete) |
| [`plans/2026-08-30-ssh-dhcp-plan.md`](plans/2026-08-30-ssh-dhcp-plan.md) | SSH + DHCP | **Build** (complete) |
| [`plans/2026-08-30-devices-plan.md`](plans/2026-08-30-devices-plan.md) | Devices + Unix floor | **Build** (complete) |
| [`plans/2026-08-30-wants-fetch-plan.md`](plans/2026-08-30-wants-fetch-plan.md) | wants + fetch | **Build** (complete) |
| [`plans/2026-08-30-display-plan.md`](plans/2026-08-30-display-plan.md) | Display canary | **Build** (complete) |
| [`plans/2026-08-30-sola-river-plan.md`](plans/2026-08-30-sola-river-plan.md) | T21 River first | **Build** (complete) |
| [`plans/2026-08-30-libinput-plan.md`](plans/2026-08-30-libinput-plan.md) | T22 libinput without udev | **Build** (complete) |
| [`plans/2026-08-30-oath-sola-plan.md`](plans/2026-08-30-oath-sola-plan.md) | T23 oath-sola session | **Build** (complete) |
| [`plans/2026-08-31-sola-session-plan.md`](plans/2026-08-31-sola-session-plan.md) | T25 sola-session | **Build** (complete) |
| [`plans/2026-08-31-sola-terminal-plan.md`](plans/2026-08-31-sola-terminal-plan.md) | T26 sola-terminal | **Build** (complete) |
| [`plans/2026-08-31-metal-canto-plan.md`](plans/2026-08-31-metal-canto-plan.md) | T27 metal canary | **Build** (complete) |
| [`plans/2026-09-01-sola-browser-plan.md`](plans/2026-09-01-sola-browser-plan.md) | T28 sola-browser | **Build** (complete) |
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
| `forks/` | Maintained source forks (submodules; build-time) |
| `apps/` | First-party `pkg:*` sources (`hello`, `fetchme`) |
| `apocrypha/` | Scratch / history — not built, gitignored |
| `.grok/skills/` | `oath-session-start`, `oath-progress-docs` |
| `.grok/rules/active-work.md` | **Pointer** to `CURRENT.md` |

## Authority order

1. **Code that ships** (and tests)
2. **Root `CURRENT.md`** for active priority and dogfood facts
3. **`docs/capabilities.md`** for capability maturity
4. This `docs/` suite for intent and map
5. `apocrypha/` — ignore unless hunting history
