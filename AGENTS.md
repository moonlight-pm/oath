# Oath — contributor and agent guide

Oath is a **new Linux distribution**: the Linux kernel and our own
userspace, not a remix of Debian, Arch, NixOS, Alpine or Ubuntu.
Principles: **independence, simplicity, openness, courage.**

The product is **AI-first**: the sysadmin is an agent. A running
machine must teach a model that has never seen Oath where things are
and how to change them. Its identity is a typed, live catalog at
`/oath` and one admin surface, `oath`, which humans and agents share.
We write PID 1. Unix is the substrate, not the admin interface.

- **Is not:** a kernel project, a chatbot in PID 1, or systemd with a skin.
- **Toolchain and libc:** GCC, binutils and glibc, one of each (R17).
  Programs link dynamically through `/lib`; Oath's own binaries are
  static.
- **Packages:** native only — built from source by an Oath `plan`, or
  the vendor's own release pinned by sha256. Never another distro's
  packages (R7 in [docs/decisions.md](docs/decisions.md)).
- **License:** MIT, copyright Joshua Kifer. Upstream files keep theirs.

## Session start

1. This file.
2. [CURRENT.md](CURRENT.md) — the slice, the machines, the don'ts.
3. [docs/decisions.md](docs/decisions.md) — if your slice touches an
   **Open** decision, ask Joshua; do not invent product policy.
4. The spec for your domain in [docs/specs/](docs/specs/), and its row
   in [docs/capabilities.md](docs/capabilities.md).

"go", "ok go", "continue" with no new task means: do CURRENT → Now,
item 1. Do not re-plan from scratch.

## Docs (same change as the code)

| File | Holds | Rule |
|------|-------|------|
| `CURRENT.md` | priority, next steps, machines | ≤ 80 lines, ≤ 5 Now items, replace don't append |
| `docs/capabilities.md` | what works, main gaps | ≤ 800 words, one row per capability |
| `docs/decisions.md` | standing decisions, open questions | ask before acting on Open |
| `docs/specs/` | target design, one file per domain | label "is" vs "will be" |
| `docs/manual/` | how to use what ships today | shipped behavior only |
| `docs/roadmap.md` | phase status | flip only at phase level |
| `docs/architecture.md` | as-built map: processes, trees, paths | |
| `hosts/<name>.md` + `.json` | one machine's facts and install overlay | overwrite, no diary |
| `site/pages/home.html` | oath.moonlight.pm: goals, the kinds, status | Status and counts dated; follow capabilities |

Code wins over a stale doc; then fix the doc. No session diaries,
handoff files or status files beyond these. The end-of-slice checklist
is the `oath-docs` skill (`.claude/skills/oath-docs/`).

## Agent autonomy

**Do without asking:** read, search and edit project files; docs
hygiene; normal git on `master` (status, diff, log, add, commit);
`cargo test`; `cargo make build`; `cargo make check`; local QEMU of
images this repo built. **Anything on canto**, the test machine
(Joshua, 2026-10-18): `cargo make install --target home@10.0.0.3
--disk /dev/sda … --confirm`, `sudo oath update --confirm` there,
reboots, `sudo`, wiping its disk. **Anything on buccina**, where Oath
work happens (R39, Joshua 2026-10-28; canto is for Sola R&D, so keep
long Oath builds off it): installs, updates, reboots, `sudo`, wiping
its disk.

**Ask first:** force-push or hard reset of published history; writing
to any other real disk or USB stick Joshua did not ask for (`cargo make
install --confirm`, `--usb`, `sudo oath update --confirm` elsewhere);
changing novus (Joshua's desk, on Oath since 2026-10-30); committing
secrets; answering an Open decision; publishing a release.

## Tree

```
AGENTS.md  CURRENT.md  README.md  LICENSE
hosts/            one file pair per machine: facts (.md) + install overlay (.json)
docs/             decisions, capabilities, roadmap, architecture, specs/, manual/
crates/oath-core  catalog, kinds, apply/undo, seed profiles (seed/*.json)
crates/oath       the `oath` CLI
crates/oath-init  PID 1, serial-login, screen-login, sudo
crates/oath-efi   UEFI boot menu and mark
crates/oath-make  host build tool (`cargo make bootstrap|pkgs|build|check|run|up|start|stop|ssh|install|update|esp|store|publish|advisories`)
pkgs/             every package that ships: pkg.toml + build.sh per package, lock.json, the toolchain bootstrap;
                  pkgs/linux is the kernel (linux.fragment, patches/); pkgs/keys the upstream signing keys
image/            host.nix (QEMU + OVMF, build host only)
check/            cases.toml for `cargo make check` (the QEMU harness), agent.toml for `--agent`
apps/             first-party package sources and example plans
site/             oath.moonlight.pm: home page and docs/manual as one page, its own Cargo workspace, Wicket workload
brand/            the seal: oath.svg (vector), oath.png, logo-white.png
forks/            submodules of patched upstreams (river, wlroots)
apocrypha/        gitignored scratch — never a source of truth
```

Kinds are `host svc snap pkg net ssh dev plan`. Do not add a kind
without a decision in `docs/decisions.md`.

## Workflow

- Work on `master`, and push it. Small commits; product and docs together.
  An integration branch `next` only while several sessions overlay
  Oath's own packages (R47).
- **Packages and releases (R47; the user-level `oath-packages` skill).**
  A release goes out only from a clean `master` equal to `origin/master`
  (`cargo make publish` fetches and refuses otherwise; publishing is
  Joshua's word). Packs from elsewhere (Sola's) are pinned in a profile
  by `hash`, `needs` and `source`. A pin no release set is an overlay:
  `oath get` and `oath diff` name it, and `oath update` replaces it
  unless `--keep pkg:<name>`. Record an applied package with its source
  (`pkg:sola … from next@ce6408de`), not a bare hash.
- Never commit secrets.
- Do not assume Debian/Arch/NixOS conventions apply to the product.
- **Driver patches first.** A generation-gap GPU bug (SI DPM, modeset)
  is fixed in `pkgs/linux/patches/`, gated by chip family, so one
  kernel ships everywhere. Userspace workarounds only prove the hop.

## Forks

There is no Sola fork (R28): Sola packages itself from its own repo
(`moonlight-pm/Sola`) and publishes its packs, and the `desk` profile
pins them by hash (`hash`, `needs`). Oath's tools stay generic and
never name Sola. Do not edit the Sola repo from here; its own session
does that.
