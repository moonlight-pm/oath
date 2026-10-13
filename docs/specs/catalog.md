# Catalog

The catalog at `/oath` is the identity of Oath. Every configurable
thing on the box is an object named `kind:name` with a desired document
(what the owner asked for) and an actual document (what the engine
observed). `oath set` writes desired, `oath apply` takes a btrfs
generation and converges, and `oath undo` rolls back. Humans and agents
use the same verbs.

**Status:** partly built. The loop works. Writes are not yet safe
against concurrent writers or crashes, and undo is wrong after a failed
apply. Those fixes are the target below.

Replaces: T1 (catalog and `oath` surface), the catalog parts of T7, T24,
T34.

## Tree (built)

```text
/oath/
  INDEX.md                  generated; the agent starts here
  schema/<kind>.json        JSON Schema 2020-12
  schema/<kind>.md          prompt-sized prose: purpose, fields, safety, example
  objects/<kind>/<name>/
    desired.json            owner intent
    actual.json             engine-owned observation
    meta.json               id, safety, status, last generation
    applied.json            svc only: last desired that converged
  log/apply.jsonl           one line per apply or undo
  run/                      sockets; init.sock; fs/ (btrfs top-level mount)
  store/                    packages and plans (see packages.md)
  ssh/                      host keys
```

Documents are pretty-printed UTF-8 JSON with a trailing newline. A kind
exists only if `schema/<kind>.json` exists. Today's kinds: `host`,
`svc`, `snap`, `pkg`, `net`, `ssh`, `dev`, `plan`. No kind is added
without a recorded decision.

`status` in `meta.json` is `in-sync`, `drift`, `applying` or `error`.

## Identity

An id is `kind:name`. There is exactly one `host:local`.

Target grammar (not enforced yet): kind `[a-z][a-z0-9-]*`, name
`[a-z0-9][a-z0-9.-]*`. Today the parser only splits on `:` and checks
that both halves are non-empty.

## Verbs (built)

| Verb | Does |
|------|------|
| `oath` | Short INDEX and the path to the full one |
| `oath ls [--kind k]` | Ids |
| `oath schema [kind]` | Kinds, or one kind's schema and prose |
| `oath get <id> [--desired\|--actual]` | Desired, actual, meta |
| `oath set <id> k=v…` / `--from-json` / `--from-file` | Merge into desired |
| `oath diff [id]` | Drift; exit 0 when none |
| `oath apply [id…] [--confirm]` | Generation, then converge |
| `oath undo` | Roll back the last apply |
| `oath log` | Apply log |
| `oath build`, `oath fmt` | Plans (see builds.md) |

`--json` prints the same facts as JSON. Errors carry a `hint` that
names a verb or schema page. Exit codes: 0 ok, 1 error, 3 `--confirm`
required, 4 security refusal.

`oath seed` is hidden. It writes a fresh catalog from a built-in
profile, or merges overlay files into an existing catalog. It is used
at build and install time only (see boot-and-install.md).

## Kinds owned here

**`host:local`** — `hostname`, `power` (`run`, `reboot`, `halt`),
`env`, `timezone`. `env` is injected into every service and written to
a root-owned `/etc/profile`. `timezone` is a POSIX TZ string; when set,
every service and login gets `TZ` from it unless `env.TZ` is set. The
system clock stays UTC, and recorded timestamps are RFC 3339 with `Z`.
A `power` other than `run` needs `--confirm`.

**`snap:current`** — the live generation number. Changing it directly
needs `--confirm`; `oath undo` is the normal path.

Other kinds: `svc` in services.md, `pkg` in packages.md, `plan` in
builds.md, `net` and `ssh` in network-and-ssh.md, `dev` in devices.md.

## Apply and undo (built)

`oath apply`:

1. Compute drift for the selected ids (or all). Nothing to do: exit 0.
2. If any change is confirm-class and `--confirm` is absent: exit 3.
3. Check the `svc` wants graph and the `pkg` needs graph.
4. Snapshot the live `@` subvolume as `@gen-N` from the top-level mount.
5. Converge each object through its kind handler; refresh actual.
6. Append a log line: time, actor (uid, tty), ids, generation, parent,
   result. Regenerate INDEX.

`oath undo` finds the last log line with `result: ok`, restores
`objects/`, `schema/`, `log/`, `INDEX.md`, `store/` and `ssh/` from its
parent generation, and converges.

Known defects, from the 2026-10-13 review:

- Files are rewritten in place with no lock. A reader can see half a
  file, and PID 1 stops any service whose desired it cannot parse.
- A failed apply writes no log line, so undo reverts the previous
  successful apply instead.
- Typed validation happens after the snapshot is taken.
- Undo copies `log/` and `store/` back, so it erases history and can
  pull package trees out from under running processes.
- `set` checks key names only, not types.
- Nothing can create or delete an object except the seed.
- A missing `INDEX.md` at boot makes PID 1 reseed the catalog.

## Target

- **Writes (proposed, awaiting the owner — D4).** Every writer (`oath`
  and PID 1) takes an exclusive `flock` on `/oath/.lock`, writes to a
  temporary file, fsyncs, and renames over the target. Readers never
  see a partial document. The alternative, PID 1 as the only writer,
  is rejected unless the owner prefers it, because `oath --root /mnt`
  must work with no PID 1.
- **State split.** Spec (desired documents) is what generations and
  undo restore. Status (actual, meta) is engine-owned and recomputed.
  The log is append-only and never rolled back. The store is
  content-addressed and never rolled back; a generation pins hashes.
- **Validate first.** Parse every selected desired into its Rust type
  and check every graph before allocating a generation.
- **Log every outcome.** `ok`, `error` and `refused` all get a line.
  Undo reverts the most recent apply, whatever its result.
- **Apply is the commit point.** PID 1 reads `applied`, never
  `desired`, so an un-applied `set` cannot change a running service.
- **Create and delete.** `oath new <id>` makes an object from schema
  defaults; `oath rm <id>` deletes one. Both go through apply.
- **Typed `set`.** Values are parsed against the kind's type. Schemas
  are generated from the Rust types and embedded, so code and schema
  cannot drift.
- **Id grammar** as above, enforced at parse.
- **Probation apply.** Changes that can lock the owner out (`ssh:local`,
  `svc:sshd`, `net:net0`) apply on a timer and roll back unless
  confirmed from a new session. Mechanism to be designed.
- **Boot never reseeds** an existing catalog. A missing INDEX is
  regenerated.

## INDEX

`INDEX.md` is generated from the live catalog. It says, in order: you
are on Oath, not another distro; do not edit `/etc`, use `oath`; the
verbs; the kinds present with a pointer to `oath schema <kind>`; safety
(apply snapshots, `--confirm` only when the owner asked); how you got
here. Bare `oath` prints a short form and the path.

## Courage test

On a fresh install, an agent that has never seen Oath reads INDEX,
runs `oath ls`, `oath schema host`, `oath get host:local`, sets a new
hostname, diffs, applies, reboots, sees the name in actual, and undoes
it. Target additions: a torn file, a failed apply followed by undo, and
a lockout attempt each behave as described above.
