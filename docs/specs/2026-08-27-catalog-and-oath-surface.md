**Date:** 2026-08-27
**Status:** target (freeze)
**Implementation:** partial
**Dogfood:** QEMU probe (`oath-make probe`) — hostname apply, undo, confirm-reboot, hostname survives reboot
**Gaps:**
- MCP, extra kinds (out of freeze)
- Boot-generation picker (undo is the supported rewind)
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Catalog, objects, and `oath` — target design

This is the identity of Oath. Init, packages, devices, and images hang off
this model. They do not invent a parallel one.

Normative desired shape. Not a changelog. Not an implementation checklist
(that is [../plans/2026-08-27-qemu-skeleton-plan.md](../plans/2026-08-27-qemu-skeleton-plan.md)).

---

## Locks this freeze owns

- Live catalog tree is **`/oath`**. Agents start at `/oath/INDEX.md` and
  do not hunt `/etc`.
- Object identity is **`kind:name`** (no UUIDs in v0).
- Documents are **pretty-printed JSON** the agent can `cat`. Schemas are
  **JSON Schema 2020-12**. Kind prose is Markdown.
- Desired and actual are **separate files**. Do not mix is and will-be.
- One admin surface: **`oath`**. Text by default, **`--json`** as the
  same view. MCP is a later adapter, not a third ontology.
- v0 kinds: **`host`**, **`svc`**, **`snap`**. Nothing else in Phase 1.
- **`oath set`** writes desired only. **`oath apply`** snapshots, then
  converges. **`oath undo`** rolls the last apply.
- Root filesystem is **btrfs**. Generations are subvolume snapshots.
- PID 1 is ours. **`svc` objects are its only config.**
- Safety: `mutate` applies with a snapshot; `confirm` refuses without
  `--confirm`. Halt, wipe, and boot-generation changes (other than undo
  of the last apply) are `confirm`. The INDEX tells agents not to pass
  `--confirm` unless the owner asked for that class of change.
- Single seat: **root is the owner**. The agent is not a second Unix
  user. Apply log records actor (uid, tty).
- Dogfood: **x86_64 QEMU**, **serial console** is how the agent reaches
  the box. SSH is later. No desktop, no installer in Phase 1.

---

## Courage test (definition of done for this freeze, once implemented)

On a QEMU appliance, an agent that has never seen Oath:

1. Reads `/oath/INDEX.md` (or `oath` with no args).
2. `oath ls` · `oath schema host` · `oath get host:local`
3. `oath set host:local hostname=<new>` · `oath diff` · `oath apply`
4. Reboots (serial still works).
5. `oath get host:local` shows the new name in **actual**.
6. `oath undo` restores the previous generation.

If that loop works, the catalog is real. A package archive without this
loop is not Oath.

---

## Tree

```text
/oath/
  INDEX.md                 generated. agent starts here. always.
  schema/
    <kind>.json            JSON Schema
    <kind>.md              prompt-sized: purpose, when not to use, examples, safety
  objects/
    <kind>/<name>/
      desired.json
      actual.json
      meta.json            id, safety, status, last generation
  log/
    apply.jsonl            one JSON object per apply / undo
  run/                     sockets, pid files; not desired state
```

Compatibility shard (`/bin`, `/usr`, `/proc`, …) exists to *execute*
what we shipped. It is not where agents look for truth. **Never** teach
an agent to edit `/etc`. If a foreign binary requires a file there,
`oath apply` may write it as a side effect of converging an object, and
the catalog remains the source of truth.

`/oath` lives on the root btrfs subvolume so it is included in
generations.

---

## INDEX

`/oath/INDEX.md` is **generated from the live catalog** so it cannot
advertise kinds that are not there. Regenerated on apply and at boot.

It must say, in this order, in a size a model can load:

1. You are on **Oath**. Do not assume Debian, Arch, NixOS, Alpine, or
   Ubuntu.
2. Do not edit `/etc` or random files. Use `oath`.
3. The verbs (below) and `oath --help`.
4. Kinds present, one line each, pointer to `oath schema <kind>`.
5. Safety: apply snapshots first; `--confirm` is owner-class only.
6. How you got here (serial on the appliance).

`oath` with no arguments prints a short INDEX and the path to the full
file.

---

## Identity and records

An id is `kind:name`. Kind is `[a-z][a-z0-9-]*`. Name is
`[a-z0-9][a-z0-9.-]*`. Examples: `host:local`, `svc:serial`, `snap:3`.

There is exactly one `host:local` on a machine.

`meta.json`:

```json
{
  "id": "host:local",
  "kind": "host",
  "name": "local",
  "safety": "mutate",
  "status": "in-sync"
}
```

`status` is `in-sync` | `drift` | `applying` | `error`.

`desired.json` / `actual.json` are the kind’s schema, no envelope.

On-disk JSON is pretty-printed UTF-8 with a trailing newline. The agent
can `cat` any of these files; `oath get` is the supported way.

---

## Verbs

One ontology. Human text on stdout. `--json` emits the same facts as
JSON. Errors in `--json` are objects `{ "error": "...", "hint": "..." }`
and **always** hint a catalog page or verb.

| Verb | Does |
|------|------|
| `oath` | Short INDEX |
| `oath ls` | Ids, optional `--kind` |
| `oath schema [kind]` | All kinds, or schema + prose for one |
| `oath get <id>` | Desired + actual + meta. `--actual` / `--desired` to clip |
| `oath set <id> k=v...` | Merge into desired. Also `--json '{...}'` |
| `oath diff [id]` | Drift. Exit 0 if none |
| `oath apply [id...]` | Snapshot, converge listed ids or all drift |
| `oath undo` | Restore the generation created by the last apply |
| `oath log` | Apply log, newest last |

`set` does not converge. `diff` does not write. `apply` is the only
mutate of actual (plus `undo`).

`--confirm` is required when the apply set includes any `safety:
confirm` object or field. Without it: exit 3, print what would happen,
hint `--confirm` and the INDEX safety paragraph. Agents must not pass
`--confirm` unless the owner asked.

Exit codes: `0` ok, `1` error, `3` confirm required.

---

## Apply, generations, undo

Root is a **btrfs** subvolume (names are an implementation detail of
the boot plan). `oath apply`:

1. Compute drift for the selected ids. Empty → exit 0.
2. If any selected change is `confirm` and `--confirm` is absent →
   exit 3.
3. Snapshot the live root subvolume. Allocate the next generation
   number `N`. Record it.
4. Set `status=applying`. Converge each kind handler. Refresh actual.
5. Append `/oath/log/apply.jsonl`. Set `status` to `in-sync` or
   `error`.
6. Regenerate `INDEX.md`.

Apply log line (fields): `time`, `actor` (uid, tty), `ids`,
`generation`, `parent_generation`, `result` (`ok` | `error`),
`error` if any.

**Undo:** restore the snapshot taken for the last successful apply
(files *and* in-memory bits the handlers know how to revert, e.g.
hostname). Undo of last apply is `mutate`, not `confirm`. Selecting an
older generation as the boot default is `confirm`.

qcow2 snapshots on the QEMU *host* are debug only. They are not this
mechanism.

If apply fails mid-way, leave `status=error`, keep the snapshot, do not
silently auto-undo. The agent can `oath undo` or fix desired and apply
again.

---

## Kind: `host`

Singleton `host:local`. Safety: `mutate` for hostname; `confirm` for
`power` (`reboot` | `halt`).

**desired / actual (v0):**

```json
{
  "hostname": "oath",
  "power": "run"
}
```

- `hostname` is a Linux hostname (not an FQDN requirement in v0).
- Persist in the catalog. On boot, PID 1 (or `oath apply` at boot)
  sets the kernel hostname from desired **before** starting services.
- Optional compatibility write of `/etc/hostname` is a side effect,
  never the source of truth.
- `power: run` is steady state. `oath set host:local power=reboot`
  plus `oath apply --confirm` reboots after converge.

---

## Kind: `svc`

One object per supervised process. Safety: `mutate`.

PID 1 is **not** an object. It is the engine that converges `svc:*`.

**desired (v0):**

```json
{
  "exec": ["/usr/lib/oath/serial-login"],
  "wants": [],
  "restart": "always",
  "enabled": true
}
```

- `exec`: argv, absolute paths.
- `wants`: other `svc` ids that should be up first. No cycles.
- `restart`: `never` | `always` | `on-failure`.
- `enabled`: if false, the process is not running.

**actual (v0):**

```json
{
  "state": "running",
  "pid": 42,
  "restarts": 0
}
```

`state`: `stopped` | `starting` | `running` | `failed`.

Init reads desired from `/oath/objects/svc/*/desired.json`. There is no
unit file, no init.d, no systemd. `oath apply` on a `svc` writes
desired (already), then **notifies PID 1** to converge (Unix socket
under `/oath/run/`, protocol is an implementation detail; it is not a
second config format). On boot, PID 1 converges all enabled `svc`
after `host`.

Phase 1 image ships at least `svc:serial` — a login or shell on the
QEMU serial so an agent can type `oath`.

---

## Kind: `snap`

Immutable records `snap:<n>` (read-only actual) plus singleton
`snap:current`.

**`snap:current` desired / actual:**

```json
{
  "generation": 3
}
```

Changing `snap:current` to anything other than “undo last apply” is
`confirm` (boot-generation class). `oath undo` is the supported mutate
path.

**`snap:<n>` actual (no desired):**

```json
{
  "generation": 3,
  "parent": 2,
  "time": "2026-08-27T00:00:00Z",
  "reason": "apply host:local"
}
```

---

## Kind prose (required per kind)

Each `<kind>.md` is prompt-sized and says:

- What this kind is
- When to use it / when not
- Fields that matter
- Side effects (reboot? network blip? snapshot?)
- Safety class
- A copy-paste example of `get` / `set` / `apply`

If a kind is not in `/oath/schema`, it does not exist.

---

## Principals

One seat. Serial login is **root** (the owner identity). The agent is
whoever is on that tty — human or model — running `oath`. No second
Unix user in Phase 1. The apply log’s `actor` is uid + tty.

---

## Out of this freeze

Do not sneak these in under catalog work:

- Package objects, devices, a real network kind, glibc runtimes
- MCP
- SSH
- Disk installer, bare metal, aarch64
- Busybox-vs-own-coreutils as identity (borrowed busybox is allowed
  in Phase 1 as a prebuilt; it is not the catalog)

A later freeze extends kinds. It does not replace this surface.

---

## Errors and discoverability

Every failure path points at the next page: missing id → `oath ls`;
unknown field → `oath schema <kind>`; confirm required → INDEX safety
paragraph. The system teaches itself. Do not emit Debian-shaped advice.
