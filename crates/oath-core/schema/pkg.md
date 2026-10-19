# pkg

A package. Bits live under `/oath/store/pkg/<name>/<hash>/`. `/bin` is
a symlink farm, not an unpack target. There is no `apt` and no
`oath install`. Name is a slot; hash is the bits.

**When to use:** install or remove a package (`present`); pin a
realization (`hash`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not set `present=false` on the base's fixed packages
(`pkg:glibc`, `pkg:bash`, `pkg:coreutils`, `pkg:util-linux`,
`pkg:iproute2`, `pkg:dhcpcd`, `pkg:btrfs`, `pkg:oath`,
`pkg:dropbear`); they cannot be removed either. Create a
`pkg` object with `oath new pkg:<name>` (`present` defaults to true)
and remove one with `oath rm`
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
- `file` — optional, with `url`: the URL is one file, a vendor's
  release, and the tree is that file alone at this relative path, not
  executable (`src/claude`). A recipe's source package (below).
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
`hash`, `components`, optional `advisories`, `realizations` (other
trees for this name), and `verified` (`ok`, `at`; `got` when the tree
no longer hashes to `hash`). If `removable` is false, `present=false`
is refused (not `--confirm`).

Integrity: apply hashes the tree again before linking and refuses a
changed one. `oath verify` re-hashes every linked tree (PID 1 runs it
after each boot); a damaged package has status `error` and INDEX lists
it. Fetch it again (`url`) or reinstall.

Safety: `mutate`. Apply snapshots, then converges links.

## Example

```
oath ls --kind pkg
oath get pkg:dosfstools
oath set pkg:dosfstools present=false
oath apply pkg:dosfstools
oath set pkg:dosfstools present=true
oath apply pkg:dosfstools
oath undo
```

The `base` profile ships every package present: the fixed ones above,
one GNU tool for each job (coreutils, sed, grep, gawk, findutils,
diffutils, tar, gzip, xz, bzip2, procps-ng, less, vim), ping, chrony,
OpenSSH's client tools, dosfstools, curl, dig, host and nslookup
(`pkg:bind`), openssl, and the libraries they load.
A library a present package needs cannot be removed: apply refuses a
broken `needs` graph. All but `pkg:oath` (built by cargo) come from
plans in `pkgs/`, built from source with GCC against glibc. Programs
load their libraries from `/lib`, a link farm like `/bin`: a package's
`lib/` entries are linked there when it is present. There is no C
compiler on the box yet.

**Recipes** (R23): a package whose licence does not allow
redistribution is not in the image. The catalog has its plan
(`plan:<name>`), its source (`pkg:<name>-src`, the vendor's file by
`url` and `file`) and `pkg:<name>` absent at the pin the build host
got; INDEX lists it under Recipes. Build it here with `sudo oath build
plan:<name>`, then `oath set pkg:<name> present=true` and apply: the
pin refuses a pack that is not the build host's. `pkg:claude-code` is
one (`claude`).
