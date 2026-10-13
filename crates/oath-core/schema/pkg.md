# pkg

A package. Bits live under `/oath/store/pkg/<name>/<hash>/`. `/bin` is
a symlink farm, not an unpack target. There is no `apt` and no
`oath install`. Name is a slot; hash is the bits.

**When to use:** install or remove a package (`present`); pin a
realization (`hash`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not set `present=false` on `pkg:busybox`, `pkg:btrfs`,
`pkg:oath`, or `pkg:dropbear`; they cannot be removed either. Create a
`pkg` object with `oath new pkg:<name>` and remove one with `oath rm`
(both take effect at `oath apply`); `oath build` creates the product's
object, and an install overlay can add others.

## Fields

- `present` — `true` links `store/.../<hash>/bin/*` into `/bin`.
  `false` removes **this object's** links. The store tree stays, so a
  re-install needs no network.
- `hash` — optional pin. Realization id is `sha256-` plus 64 lowercase
  hex of the pack tree (`oath-tree-v1`). Apply verifies the bytes
  match; it does not choose. Empty hash is discovery: the unique
  on-disk tree, or a fetch, reports `actual.hash`. Two hashes of one
  name can sit on disk; only the pin is linked.
- `url` — optional. If set and the store is missing, apply fetches it
  then links. `oath build` fetches a missing `build_needs` pack the
  same way before the sandbox starts (it does not set `present`). A
  `.tar` (or `.tar.gz`) is a pack tree; the hash is of the tree, not
  the tar. An origin prefix plus `hash` fetches
  `{url}/pkg/{name}/{hash}.tar`. A single file becomes `bin/<name>`.
  Local store wins if the realization is already there.
- `needs` — optional list of `{id, hash}` this pack needs at runtime.
  `hash` is required. Cycles refuse apply. `present=true` is refused
  if a need is missing or its live pin differs. `present=false` is
  refused while another **present** pack still needs this. Apply does
  not rewrite other objects.
- `description` — short prose. Empty means none.
- `home` — HTTPS URL of the source or project page. Not `url`.

Pack tree (not desired; part of the hash): `components.json` lists
upstream `{name, version, purl}` (optional `cpe`). Missing means
unknown, not safe. Actual copies the live tree's list.

Advisories (not desired; not in the pack):
`/oath/store/adv/<name>/<hash>.json`, fetched from
`{origin}/adv/<name>/<hash>.json` when `url` is an origin. A failed
fetch does not refuse apply. Actual `advisories` is omitted when never
fetched. An empty list is not a safety proof. The box never calls
OSV or NVD.

The slot has `live` → `<hash>` and compatibility links (`bin` →
`live/bin`, …) that are not part of the hash.

Actual also has `links` (basenames in `/bin`), `removable`, live
`hash`, `components`, optional `advisories`, and `realizations` (other
trees for this name). If `removable` is false, `present=false` is
refused (not `--confirm`).

Safety: `mutate`. Apply snapshots, then converges links.

## Example

```
oath ls --kind pkg
oath get pkg:hello
oath set pkg:hello present=true
oath apply
hello
oath set pkg:hello present=false
oath apply
oath undo
```

The `base` profile ships `pkg:busybox`, `pkg:btrfs`, `pkg:oath`, and
`pkg:dropbear` present and not removable; `pkg:curl` present and
removable; `pkg:hello` absent with its bits already in the store
(`/bin/hello` prints `hello`); and `pkg:fetchme` absent with a `url`
on the QEMU host. Busybox applets are one package. There is no glibc and
no C compiler on the box yet.
