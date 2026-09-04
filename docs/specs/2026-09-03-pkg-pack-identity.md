**Date:** 2026-09-03
**Status:** target (freeze)
**Implementation:** not started
**Dogfood:** none
**Gaps:**
- store path is still `/oath/store/pkg/<name>/` (no hash component)
- no `hash` field on `pkg` desired/actual
- `oath get` does not list realizations
- no pack-from-path helper (host `cargo make` copies trees; no hash print)
- guest store export still T20
- hash function + canonical tree encoding not chosen (must be
  deterministic and written into `schema/pkg.md` when shipped)
- signatures / signed indexes still out
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Pack identity (content hash, no recipe language)

Extends [2026-08-28-packages.md](2026-08-28-packages.md) (T11/T12),
[2026-08-30-pkg-hosting.md](2026-08-30-pkg-hosting.md) (T20),
[2026-08-31-sola-dev.md](2026-08-31-sola-dev.md) (T24).
Does not replace them. Same verbs. No new kind.

A pack is a directory that matches what Oath already stores. Authors
build that directory however they want (`make pack`, a script, copying
files). Oath does not ship a recipe language. The running system
hashes the tree; that hash is the realization. Git is optional
provenance, not the version.

---

## Locks this freeze owns

- **Name is a slot. Hash is the bits.** `pkg:<name>` is the catalog
  object and the `/bin` farm it owns. Two trees that both claim
  `pkg:foo` can be unrelated programs. Oath does not infer “the right
  `foo`” from the name.
- **Realization id is the content hash of the packed tree.** Same
  bits, same hash. Different bits, different hash. There is no
  overwrite of a hash with other contents. There is no `unknown` tag.
  A dirty working copy hashes as whatever tree you packed, not as
  HEAD.
- **Store path (amends T11):**
  `/oath/store/pkg/<name>/<hash>/`. Extra hashes for the same name
  stay on disk. `/bin` links target the live hash’s `bin/` files.
- **One live occupant.** `desired.hash` is the pin. Apply links that
  realization (and verifies bytes match the pin). `actual.hash` is
  what is linked. Switching hash is `oath set pkg:foo hash=…` then
  `oath apply`. `oath undo` is still the way back (T24 generations).
- **Two runnable at once is still two names** (T24): `pkg:foo` and
  `pkg:foo-wip`. Hash-in-path keeps extra realizations; it does not
  put two `/bin/foo` on PATH.
- **No recipe language. No new kind. No new verbs.** Pack guidelines
  are the layout below. Fill is pack-from-path, or `pkg.url` (T20)
  whose bytes must match `desired.hash`. Git is not the store; apply
  does not clone, checkout, or speak refs (T20).
- **The pin is how you know the hash.** Apply verifies; it does not
  choose. There is no global correct hash for a name. You learn a
  pin from a place you already trust, then you write `desired.hash`.

---

## Pack layout (guidelines)

A pack is a directory that will live at
`/oath/store/pkg/<name>/<hash>/`:

```text
bin/          # each file → /bin/<basename> when this object is present
lib/          # optional; not on PATH
share/        # optional
INDEX.md      # optional; short, agent-readable: what this is, how to run it
```

- Do not exec from the store; `/bin` is how you run what is installed.
- How you produce the tree is yours. First-party `apps/<name>/` is
  already this shape. Host packing (`cargo make`, relocate scripts)
  already emits it.
- Optional `INDEX.md` is how a human or agent tells two hashes of
  the same name apart (they may be different programs). Oath will
  not invent a second description format.

---

## Catalog and tooling

Same surface as today: `oath ls`, `oath get`, `oath set`, `oath apply`,
`oath undo`. No `oath install`.

**desired (v0 + this freeze):** `present`, optional `url` (T18/T20),
optional `hash` (the pin).

**actual:** `present`, `links`, `removable`, live `hash`, and a
summary of other realizations on disk for this name (hash, whether
linked, `bin/` names, first lines of pack `INDEX.md` if present).

`oath get pkg:foo` is the examination surface. To see that two hashes
are not the same thing: read each tree (and its `INDEX.md`). `get`
summarizes; it does not call them versions of one program.

Choose:

```
oath set pkg:foo hash=<hash>
oath apply
```

Fill from a working tree (no commit): write the directory into
`store/pkg/<name>/<hash>/` (hash of that tree), set `present` +
`hash`, apply. Fill from a peer: set `url` and `hash`, apply wget’s
the bytes and refuses a mismatch.

`url` without `hash` may fetch and *report* the hash (today’s
`pkg:fetchme` shape). That is discovery, not a pin. A later apply
that claims a hash must match.

---

## How the supposed hash is known

The hash is a digest of the tree. Anyone with the directory can
recompute it. The “supposed to” value is `desired.hash`.

| You did this | The pin came from |
|--------------|-------------------|
| Packed a tree here | stdout of pack / hash of the directory you wrote |
| Asked a peer | their catalog: `oath get pkg:foo` (or their advertised store listing) |
| Fetched by URL | only if the URL or the peer’s actual **stated** the hash first |

The first time you see a name there is no prior pin. That first `set`
is the trust act (your pack, a box you SSH to, a pasted digest).
After that, mismatch is an error, not a silent upgrade.

A signature on “this host attests these bits” is later (T20). It
still does not make the name mean one program.

---

## Courage test (when implemented)

1. Pack a tree. Printed hash equals a recompute of that tree. Path is
   `/oath/store/pkg/<name>/<hash>/`.
2. `oath get pkg:<name>` shows `desired.hash`, `actual.hash`, links
   into that hash’s `bin/`, and any other realizations on disk.
3. A second distinct tree under the same name gets a different hash.
   Both trees remain. Only `desired.hash` is linked in `/bin`.
4. `oath set pkg:<name> hash=<other>` · `oath apply` retargets `/bin`;
   the previous tree stays. `oath undo` restores the previous pin.
5. Apply with `url` + wrong `hash` refuses. Bytes that match the pin
   link.
6. Two names (`pkg:foo`, `pkg:foo-wip`) can both be present. A `/bin`
   collision still refuses (T11).

---

## Out

- Recipe language, flakes, `make` as a guest package manager
- Git commit as version; apply that clones or checks out
- `unknown` / dirty-HEAD overwrite of a hash
- New kind (`repo`, `src`) or new verbs (`oath install`, `oath pack`
  as a guest verb — a host helper may print a hash)
- Reaping unused realizations
- Hash function bikeshed in this freeze (implementation + schema
  when shipped)
- Signatures, canonical archive, peer discovery (T20)
- Package dependencies
- Changing T24 “two runnable = two names”

## Amends

- **T11** store path gains `/<hash>/`. `/bin` farm and no second PATH
  stay.
- **T20** content-hash identity is this freeze. Serving `/oath/store`,
  signatures, and discovery stay T20 deferred.
- **T24** extra realizations of one name may sit in the store; two
  runnable still two names.
