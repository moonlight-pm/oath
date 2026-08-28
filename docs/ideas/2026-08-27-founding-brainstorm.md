# Founding brainstorm — what Oath is

**Status:** idea (historical argument, 2026-08-27). Do not implement from
this file. Founding locks and the catalog freeze superseded it.
**Related:** [open-questions.md](../open-questions.md), freeze
[../specs/2026-08-27-catalog-and-oath-surface.md](../specs/2026-08-27-catalog-and-oath-surface.md).

This is the argument, not the spec.

---

## The problem

Every existing Linux distro assumes one of:

1. A human who already knows it (Gentoo, NixOS, Arch).
2. A human who can search the internet (Ubuntu, Fedora).
3. Training data: years of Stack Overflow, man pages, and blog posts the
   model memorized.

Oath will have (3) for a long time. So the operator — an AI agent — cannot
be asked to “just use Linux.” If the agent has to grep `/etc`, pick among
twelve network stacks, and guess which file is live, it will hallucinate a
Debian.

**The running system has to be the training data.**

That is the whole product.

---

## Principles, as constraints

**Independence.** Not a remix. We may take *ideas* from Alpine, Void,
Chimera, Nix, Plan 9, systemd, OSTree, systemd-sysext, whatever. We do not
take their identity: not their archives, not their module language, not
their init as the OS. Tools used to produce images are not the OS.

**Simplicity.** Not “fewer packages.” One ontology. A small number of
kinds of thing, one way to inspect them, one way to change them, one place
to look first. Nix is theoretically simple and practically a maze; we want
the opposite shape: a maze-less theory that fits in a prompt.

**Openness.** The admin protocol and the catalog are public schemas. Any
agent can speak them. We do not hide the system behind a vendor chat API.
Source is open. License is MIT (see [`LICENSE`](../../LICENSE)).

**Courage.** Ship a small complete loop before a large incomplete distro.
Refuse systemd, glibc, and FHS *as identity* if they fight discoverability.
Be unusable as a general desktop for a long time. Let an agent change
hostname and have that survive reboot before we have a browser.

---

## Thesis

**Oath is a Linux whose source of truth is a typed object catalog with a
page the agent always reads first. Unix is the implementation substrate,
not the user interface.**

- PID 1 starts the machine.
- The **INDEX** starts the agent.
- `oath` is the only admin surface. Humans and agents use the same verbs.
- There is no parallel Ansible / Nix / bash folklore for “real” admin.
- Natural language is not an OS API. Structured verbs + schemas are.
  The agent speaks Oath; Oath does not parse English in PID 1.

“AI-first” means administration is not hand-edited config. It does **not**
mean the kernel is generated from vibes, or that we refuse to write the
control plane in Rust. The control plane is code. The *operating* of the
machine is an agent applying objects.

---

## Why Unix is a bad LLM API

Unstructured text. Five tools that do the same job. Implicit state
(`systemctl` vs the unit file vs the drop-in vs the generator). Conventions
that only exist in blog posts. Side effects that are not declared.

A model with Debian in its weights will emit `apt` and `systemctl` on Oath
unless the live system forbids that path and offers a better one.

So:

1. **One INDEX.** First instruction on every host: read it. It says you
   are not on Debian, here is `oath`, here are the kinds, here is what
   “never do” means.
2. **Every kind has a schema.** Name, current value, how to change,
   side effects, reversibility, examples, when *not* to use it.
3. **Prompt-sized pages.** Not a 400-page man page. An agent can load
   `oath schema net` and act.
4. **No hidden state.** If it is not in the catalog, it does not exist.
5. **Apply is explicit.** Desired vs actual. Diff. Transaction.
6. **Snapshots before mutate.** Letting an agent drive without undo is
   not courage, it is negligence.

---

## Identity is the control plane

A distro that is “independent” because it compiled 4,000 packages from
source, but is still administered with bash and `/etc`, is not Oath. A
distro that boots a tiny image whose agent can discover, change, and
verify three objects *is* Oath — even if some of those binaries were
borrowed for a while.

Independence of **design** first. Independence of **build** over time.
Waiting for a complete from-source world before the catalog exists is
how this dies in a git repo.

---

## Shape of the system

**Frozen** in
[../specs/2026-08-27-catalog-and-oath-surface.md](../specs/2026-08-27-catalog-and-oath-surface.md).
Sketch kept for the argument:

```text
/oath/INDEX          ← agent starts here. always.
/oath/schema/<kind>  ← how this kind works
/oath/objects/<id>   ← desired + actual
/oath/log            ← what changed, why, by whom

oath ls
oath schema <kind>
oath get <id>
oath set <id> <fields>
oath diff
oath apply
oath snap
```

Same surface as a CLI, as JSON, maybe later as MCP. One ontology, several
views. The CLI is not a human-only afterthought and the JSON is not an
AI-only afterthought.

Packages, devices, services, network links, users, hostname — all objects.
A package that cannot ship a schema is second-class.

Devices: `oath devices` is the inventory. The agent does not assemble
truth from `lspci`, `ip`, and udev lore.

Updates: the *base* is an image with rollback. Packages are objects on
top, not `apt upgrade`.

Init: we write PID 1. Units *are* catalog objects. The agent never
hand-writes a unit file dialect. No dinit/systemd stepping-stone.

Rollback: filesystem snapshots / generations, not a journal that is
blind to the disk. See
[2026-08-27-snapshots-and-libc-hybrid.md](2026-08-27-snapshots-and-libc-hybrid.md).

---

## What we are not

- A kernel project. Linux is the kernel.
- A remix or skin of another distro. Build tools are not the runtime.
- A chatbot OS. No English in PID 1.
- An “AI Linux” that is Ubuntu plus an LLM in the panel.
- FHS-and-glibc compatibility as a year-one goal.

---

## First complete loop (the courage test)

1. QEMU boots an Oath image.
2. An agent that has never seen Oath reads `INDEX`.
3. It lists objects, schemas hostname (or equivalent), sets it, applies.
4. Reboot. The name is still there. A snapshot exists to undo.

If that works, we have a distro. If we have a package repo and that does
not work, we have a hobby archive.

---

## Borrow, don’t inherit

Worth stealing as *ideas*:

| Source | Idea | Leave on the floor |
|--------|------|--------------------|
| Nix | Pure, inspectable builds; rollback | nixpkgs, the language, “you must learn this” |
| OSTree / A-B | Image updates, undo | Desktop-app overlay as identity |
| Plan 9 | One tree, names are the API | Research-OS purity spiral |
| systemd | Introspection, a single control plane | The unit dialect, gravity, scope |
| Alpine / Chimera | musl, small, own userspace | Being a “simple Debian” |
| Kubernetes CRDs | Typed objects + schema + apply | Clusters, YAML as lifestyle |
| Omarchy | CLI a agent can drive | Arch remix, ricing, tiling |

---

## Development method

Agents write the control plane. Humans own Decision points. Progress
docs are first-class.

The *product* is also agent-operated. Same discipline, two layers:

- **Build time:** AGENTS.md, CURRENT, specs, schemas in git.
- **Run time:** INDEX, catalog, `oath`, snapshots on the machine.

Do not confuse “the OS is not hand-configured” with “we do not write
Rust.” We write the plane. We do not write snowflake `/etc` as the
product.
