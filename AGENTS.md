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
- **Libc:** musl. glibc never shares a process with musl.
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

Code wins over a stale doc; then fix the doc. No session diaries,
handoff files or status files beyond these. The end-of-slice checklist
is the `oath-docs` skill (`.claude/skills/oath-docs/`).

## Agent autonomy

**Do without asking:** read, search and edit project files; docs
hygiene; normal git on `master` (status, diff, log, add, commit);
`cargo test`; `cargo make build`; `cargo make check`; local QEMU of
images this repo built.

**Ask first:** force-push or hard reset of published history; writing
to a real disk or USB stick Joshua did not ask for (`cargo make
install --confirm`, `--usb`); changing novus (the NixOS build host);
committing secrets; answering an Open decision.

## Tree

```
AGENTS.md  CURRENT.md  README.md  LICENSE
hosts/            one file pair per machine: facts (.md) + install overlay (.json)
docs/             decisions, capabilities, roadmap, architecture, specs/, manual/
crates/oath-core  catalog, kinds, apply/undo, seed profiles (seed/*.json)
crates/oath       the `oath` CLI
crates/oath-init  PID 1, serial-login, sudo
crates/oath-efi   UEFI boot menu and mark
crates/oath-make  host build tool (`cargo make bootstrap|pkgs|build|check|run|up|start|stop|ssh|install|esp|store|publish|advisories`)
pkgs/             every package that ships: pkg.toml + build.sh per package, lock.json, the toolchain bootstrap;
                  pkgs/linux is the kernel (linux.fragment, patches/)
image/            host.nix (QEMU + OVMF, build host only)
check/            cases.toml for `cargo make check` (the QEMU harness)
apps/             first-party package sources and example plans
forks/            submodules for the future native desk (sola, river, wlroots)
apocrypha/        gitignored scratch — never a source of truth
```

Kinds are `host svc snap pkg net ssh dev plan`. Do not add a kind
without a decision in `docs/decisions.md`.

## Workflow

- Work on `master`. Small commits; product and docs together.
- Never commit secrets.
- Do not assume Debian/Arch/NixOS conventions apply to the product.
- **Driver patches first.** A generation-gap GPU bug (SI DPM, modeset)
  is fixed in `pkgs/linux/patches/`, gated by chip family, so one
  kernel ships everywhere. Userspace workarounds only prove the hop.

## Forks

`forks/sola` is **oath-sola**; Joshua's NixOS desk runs **Sola**
(`moonlight-pm/Sola`). Pull Sola into the fork; never push
Oath-compat changes the other way. A mixed change is two commits
(generic first). Commands: [forks/README.md](forks/README.md). The
desk is out of the image until it builds natively (roadmap phase 8).
