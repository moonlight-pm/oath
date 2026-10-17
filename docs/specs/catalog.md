# Catalog

The catalog at `/oath` is the identity of Oath. Every configurable
thing on the box is an object named `kind:name` with a desired document
(what the owner asked for) and an actual document (what the engine
observed). `oath set` writes desired, `oath apply` takes a btrfs
generation and converges, and `oath undo` rolls back. Humans and agents
use the same verbs.

**Status:** partly built. Phase 3 (catalog safety) landed 2026-10-13:
locked atomic writes, typed `set`, validation before the snapshot, a
logged outcome for every apply, a spec-only stack undo, `oath new` /
`oath rm`, an id grammar and a confirm gate on changes that can cut off
SSH. Probation with automatic rollback, the store integrity check,
schemas generated from the Rust types and defaults for `oath new` were
built 2026-10-16. A physical spec/status split is still the target.

Replaces: T1 (catalog and `oath` surface), the catalog parts of T7, T24,
T34.

## Tree (built)

```text
/oath/
  INDEX.md                  generated; the agent starts here
  schema/<kind>.json        JSON Schema 2020-12, generated from the kind's Rust type
  schema/<kind>.md          prompt-sized prose: purpose, fields, safety, example
  objects/<kind>/<name>/
    desired.json            owner intent
    actual.json             engine-owned observation
    meta.json               id, safety, status, last generation
    applied.json            the spec as last applied (PID 1 runs a svc's)
    new                     marker: made by `oath new`, not applied yet
    remove                  marker: `oath rm` pending
  log/apply.jsonl           one line per apply (ok, error, refused), undo or keep
  probation.json            the apply on probation, while one is open
  .lock                     writers' exclusive flock
  .gens/<n>/objects/        copy snapshots (only off btrfs or under --root)
  run/                      sockets; init.sock; fs/ (btrfs top-level mount)
  store/                    packages and plans (see packages.md)
  ssh/                      host keys
```

Documents are pretty-printed UTF-8 JSON with a trailing newline. A kind
exists only if `schema/<kind>.json` exists.

The schemas are generated (built 2026-10-16): `oath-core` derives each
kind's schema from its desired type with `schemars`, so the doc comment
on a field is its description and serde's attributes decide what is
required and what defaults. The schema is flat (no `$ref`), closed
(`additionalProperties: false`), and leaves actual-only fields out
(`net` `lease`, `dhcp`). `oath schema`, the keys `set` accepts, and
the parse apply runs all come from the same type. The files under
`schema/` are copies for readers: seed writes them, and PID 1 rewrites
any that differ at every boot, so an upgraded `oath` brings its
schemas. The prose (`<kind>.md`) is hand-written and embedded too. Today's kinds: `host`,
`svc`, `snap`, `pkg`, `net`, `ssh`, `dev`, `plan`. No kind is added
without a recorded decision.

`status`, as `oath get` reports it, is `in-sync`, `drift`, `new`,
`removing` or `error`. Drift is desired against `applied.json` for every
kind (R14, built 2026-10-16): actual is only what the machine observed,
and neither drift nor undo reads it. An object that was never applied
drifts in every field. A catalog from before `applied.json` falls back
to actual, which used to hold the applied spec for `host`, `net`,
`ssh`, `pkg`, `plan` and `snap:current`.

Every JSON document is written to a temporary file in the same
directory, fsynced, renamed over the target, and the directory is
fsynced. A reader or a crash sees the old file or the new one, never
half. The apply log is appended and fsynced.

Every writer of spec or log takes an exclusive `flock` on
`/oath/.lock` (R4) and waits up to 30 s for another writer: `oath set`,
`new`, `rm`, `apply`, `undo`, `build`, `seed`, and PID 1 during its
boot-time catalog work. PID 1 writes svc status (`actual.json`) without
the lock; that file has one writer, and the write is atomic. Readers
(`get`, `ls`, `diff`) take no lock.

## Identity

An id is `kind:name`. There is exactly one `host:local`.

The grammar is enforced wherever an id is parsed: kind is `a-z`, at
most 16 characters; name is letters, digits, `.`, `_` and `-`, at most
64 characters, and does not start with `.`. A name becomes a directory
under `objects/<kind>/`, so it can never climb out of it.

## Verbs (built)

| Verb | Does |
|------|------|
| `oath` | The whole INDEX |
| `oath ls [--kind k]` | Ids |
| `oath schema [kind]` | Kinds, or one kind's schema and prose |
| `oath get <id> [--desired\|--actual]` | Desired, actual, meta |
| `oath set <id> k=v…` / `--from-json` / `--from-file` | Merge into desired; the result must parse as the kind's type |
| `oath new <id> k=v…` / `--from-json` | Create a `svc` or `pkg` |
| `oath rm <id>` | Mark a `svc` or `pkg` for removal |
| `oath diff [id]` | Drift; exit 0 when none |
| `oath apply [id…] [--confirm] [--probation s]` | Check, generation, then converge |
| `oath keep` | Keep the apply on probation (from a session opened after it) |
| `oath undo` | Put back the spec the newest not-undone apply changed |
| `oath verify [pkg:…]` | Hash linked package trees again (packages.md) |
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
A `power` other than `run` needs `--confirm`. A hostname is letters,
digits, `-` and `.`, at most 63 characters.

**`snap:current`** — the live generation number. Changing it directly
needs `--confirm`; `oath undo` is the normal path.

Other kinds: `svc` in services.md, `pkg` in packages.md, `plan` in
builds.md, `net` and `ssh` in network-and-ssh.md, `dev` in devices.md.

## Set, new and rm (built)

`oath set` merges fields into desired. Each key must be in the kind's
schema, and the merged document must parse as the kind's Rust type and
pass its checks: a host name; a `svc` that is enabled has an `exec`; a
`pkg` hash is `sha256-` and 64 hex digits; `net` `ipv4` is `dhcp` or
`a.b.c.d/n`; `ssh` keys start `ssh-`, `ecdsa-` or `sk-`; `snap:current`
has a numeric generation. A refusal names the fields being set.

`oath new <id>` creates a `svc` or `pkg` from the given fields (same
checks). It writes the document as the kind's type reads it, so a
field the type defaults is filled in: a `svc` needs only `exec`
(`restart: always`, `enabled: true`, `wants: []`), a `pkg` nothing
(`present: true`). It is marked `new` and does nothing
until `oath apply`. `oath rm <id>` marks a `svc` or `pkg` `removing`;
`oath apply` stops or unlinks it and deletes the object. An object that
was never applied is deleted at once. Base packages, and `host`, `net`,
`ssh`, `snap` and `dev` objects, cannot be created or removed.

## Apply and undo (built)

`oath apply`:

1. Compute drift for the selected ids (or all). Nothing to do: exit 0.
2. Check before touching anything: every selected document parses as
   its type; confirm-class changes have `--confirm`; the `svc` wants
   graph has no cycle; the `pkg` needs graph holds. A refusal takes no
   generation, changes nothing, and is logged `refused` with
   `generation: null`.
3. If the only change is `host:local` `power`, take no generation:
   converge it, log `ok` with `generation: null`, and reboot or halt.
   Power is not configuration, so undo has nothing to take back.
   Otherwise write last-good spec into the selected objects, snapshot the live
   `@` subvolume as `@gen-N` from the top-level mount, then put the
   pending spec back. The snapshot holds what was applied before.
4. Converge each object through its kind handler, write what it
   observed as actual and the spec as `applied.json` (a `pkg`'s with
   the hash it linked, so undo can put back a pin discovery found).
   svc: notify PID 1 and wait for its reply (sent after it converged),
   then check for running or stopped.
   Removals stop or unlink, then delete the object directory.
5. Log the outcome: `ok`, or `error` with the message if converging
   failed after the snapshot (the generation exists). Regenerate INDEX.

Confirm-class (exit 3 without `--confirm`): `host:local` `power` other
than `run`; `snap:current`; and changes that can cut this machine off
from SSH (lockout class) — `svc:sshd` disabled, removed, or its
`exec`, `user` or `env` changed; `ssh:local` `authorized` losing a key
(compared by key blob) or becoming empty; any change to `net:net0`
`up`, `ipv4` or `gateway`. Adding a key is not confirm-class.

## Probation (built)

A lockout-class apply goes on probation, whether it converged or
failed. Apply writes `probation.json` (generation, ids, the boot's
`boot_id`, seconds since boot when it finished, and the length:
180 s, or `--probation` 10–3600). While it is open, every other apply
is refused and logged `refused`; INDEX says so in a `## Probation`
section.

- `oath keep` ends it and logs `keep:N` (no generation; undo skips
  it). The CLI first checks that the caller is a new session: every
  ancestor process below PID 1's own children (sshd's per-connection
  process, the login shell, `sudo`) must have started after the apply.
  The session that applied cannot keep it; a fresh login proves the
  change left the machine reachable.
- `oath undo` of that generation ends it at once.
- Otherwise PID 1, which checks once a second, runs
  `oath undo --expired` when the time is up, or after the next boot
  (another `boot_id`) if the machine restarts first. That undo takes
  the lock, undoes only the apply on probation (if it is still the
  newest not-undone apply), logs `undo:N` with actor tty `probation`,
  and clears the record. A failed rollback is retried every 30 s and
  logged in `init.log`.

`oath undo` is a stack. It takes the newest log line with a generation
and result `ok` or `error` that no `undo:N` line has undone yet. From
that generation's snapshot it puts back the spec of the objects that
apply touched: `desired` and `applied`. An object the apply
created is stopped or unlinked and removed; an object it removed comes
back. It then converges those objects, sets `snap:current` to the
apply's parent and appends an `undo:N` line. Undo never touches `log/`,
`store/`, `ssh/` host keys, or objects that apply did not touch, so a
pending `set` elsewhere survives it. Status is recomputed by converging,
not copied back.

The snapshot comes from the hooks: `ApplyHooks::snapshot(n)` keeps the
catalog, and `snapshot_catalog(n)` returns the catalog root inside it
(`/oath/run/fs/@gen-N/oath` on btrfs, `<root>/.gens/<n>` for a copy
snapshot, which keeps only `objects/`).

`oath seed --overlay` (build and install time) validates each patch and
counts it as applied for `svc`, `host`, `net` and `ssh`
(`applied.json`), because PID 1 boots from applied specs. A seeded
object starts applied.

## Target

- **Physical state split.** Spec and status still share an object
  directory and both are inside the `@` snapshot; undo reads only spec.
  Status may move out of the snapshotted tree; Open in decisions.md.

## INDEX

`INDEX.md` is generated from the live catalog. It says, in order: you
are on Oath, not another distro; do not edit `/etc`, use `oath`; the
verbs; the kinds present with a pointer to `oath schema <kind>`; safety
(apply snapshots, `--confirm` only when the owner asked); how you got
here. Bare `oath` prints all of it. PID 1 regenerates a missing
`INDEX.md` at boot; it never reseeds an existing catalog (it seeds only
when `/oath/objects` is missing).

## Courage test

On a fresh install, an agent that has never seen Oath reads INDEX,
runs `oath ls`, `oath schema host`, `oath get host:local`, sets a new
hostname, diffs, applies, reboots, sees the name in actual, and undoes
it. `cargo make check` runs this loop plus a refused apply, a failed
apply followed by undo, `oath new`, `oath rm`, a lockout attempt, a
probation that rolls back and one kept from a new session, a torn file
and a wants cycle at boot (`check/cases.toml`); all pass.
