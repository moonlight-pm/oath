# pkg

A package. Bits live under `/oath/store/pkg/<name>/<hash>/`. `/bin` is
a symlink farm, not an unpack target. There is no `apt`: a package
comes by name with `oath add` (R81), and `oath install` puts this whole
system on a disk (R43). Name is a slot; hash is the bits.

**When to use:** install or remove a package (`present`); pin a
realization (`hash`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not set `present=false` on the base's fixed packages
(`pkg:glibc`, `pkg:bash`, `pkg:coreutils`, `pkg:util-linux`,
`pkg:iproute2`, `pkg:dhcpcd`, `pkg:btrfs`, `pkg:oath`, `pkg:openssh`,
`pkg:zsh`); they cannot be removed either. Create a
`pkg` object with `oath new pkg:<name>` (`present` defaults to true)
and remove one with `oath rm`
(both take effect at `oath apply`); `oath build` creates the product's
object (not with `--no-pkg`), and an install overlay can add others.

**By name (R81).** `oath ls --origin [word]` lists what the origins hold.
`oath add <name>` takes a package at its newest head a key in
`host:local.publishers` may sign for it, with each package it needs that
this machine lacks, and applies them at once. `oath new pkg:<name>` with
no fields makes the same objects without applying. Nothing the machine
has moves: a need it holds at another pin is re-pointed if the package
loses no library, and otherwise nothing is made and `oath update` comes
first. Both refuse a head older than one this machine took
(`/oath/heads.json`), as `oath update` holds it. `oath add` refuses a
name with no head on any origin. `oath new
pkg:<name>` with no fields takes one only when the store already has a
tree of it: the object gets no `hash`, and apply links that tree, as an
empty `hash` says below.

## Fields

- `present` — `true` links `store/.../<hash>/bin/*` into `/bin`, and
  removes its links to names that hash no longer has (a program the
  package dropped). `false` removes **this object's** links. The store
  tree stays, so a re-install needs no network.
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
  Local store wins if the realization is already there. Empty with a
  `hash`: the machine's `host:local.origins`, in order. A pack does
  not know where it is hosted; `url` is this machine's choice.
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
- `category` — one word for what the package mostly is. Oath's own use
  `agents`, `base`, `browser`, `desk`, `games`, `graphics`, `kernel`,
  `machines`, `network`, `security`, `server`, `sound`, `text`,
  `toolchain` and `x11`.
- `source` — optional: where the pinned tree was built from (R47),
  `{repo, commit, branch, dirty, built_by, built_at}`: `repo` as
  `https://<host>/<owner>/<name>` (no `.git`), the full 40-hex
  `commit`, `branch` (empty when detached), `dirty` (the tree had
  changes not committed), `built_by` (`user@host`), `built_at` (RFC
  3339 UTC). The tool that pins a tree sets it with the hash, in one
  apply. Not part of any hash and never a reason to move or restart
  anything: an apply that changes only `source`, `description`, `home`
  or `category` records them and touches no store, link or service.
  `null` is none. `oath update` writes the release's (or none), and
  its `description`, `home` and `category`.

Pack tree (not desired; part of the hash): `components.json` lists
upstream `{name, version, purl}` (optional `cpe`). Missing means
unknown, not safe. Actual copies the live tree's list.

Advisories (not desired; not in the pack):
`/oath/store/adv/<name>/<hash>.json`, fetched from
`{origin}/adv/<name>/<hash>.json` when `url` is an origin (every
apply), or from the `host:local.origins` entry that just served the
pack. A failed
fetch does not refuse apply. Actual `advisories` is omitted when never
fetched. An empty list is not a safety proof. The box never calls
OSV or NVD.

The slot has `live` → `<hash>` and compatibility links (`bin` →
`live/bin`, …) that are not part of the hash.

Actual also has `links` (basenames in `/bin`), `removable`, live
`hash`, `components`, optional `advisories`, `realizations` (the trees
for this name; `oath reap` says what holds each, and removes one nothing
holds), `verified` (`ok`, `at`; `got` when the tree no longer hashes to
`hash`), and the applied `url`, `needs`, `description`, `home`, `category`
and `source`.

An **overlay** is a present package whose live hash is not the head
this machine took of it (`/oath/heads.json`, R72) or, with none taken,
not the pin of the release it runs (`/oath/output.json`): `oath get`
prints an `overlay:` line (`--json`: `overlay`, with that head and its
signer, or the release's pin, and the `source`), `oath diff` lists
every one, and `oath update` replaces each unless `--keep pkg:<name>`.

A **head** is a package published on its own (R72): `pkg/<name>.json`
on an origin, signed beside itself. `oath update` moves each package to
the newest head a key in `host:local.publishers` may sign for its name,
and `oath publish pkg:<name>` puts this machine's pin up as its head. If `removable` is false, `present=false`
is refused (not `--confirm`).

A tree built on the machine reaches the store with `sudo oath store
pkg:<name> <dir|file.tar>`: it hashes the tree, copies it in (each file
0755 or 0644, as the hash records it; a special file is refused) and
prints the hash (`--json`: `{"hash": …, "id": …}`). It changes no
object and no link: `oath set pkg:<name> hash=… needs=[…]` (or `oath
new`) and `oath apply` do that. A service whose program (`exec[0]`
through `/bin`) is in this package restarts when apply moves the pin.

Integrity: apply hashes the tree again before linking and refuses a
changed one. `oath verify` re-hashes every linked tree (PID 1 runs it
after each boot); a damaged package has status `error` and INDEX lists
it. Fetch it again (`url`) or reinstall.

Safety: `mutate`. Apply snapshots, then converges links. Moving the
`hash` of the package `svc:sshd`'s `exec[0]` resolves into
(`pkg:openssh`) restarts sshd, so it needs `--confirm` and goes on
probation.

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

The `base` profile ships present: the fixed ones above, one GNU tool
for each job (coreutils, sed, grep, gawk, findutils, diffutils, tar,
gzip, xz, bzip2, procps-ng, less, vim), ping, chrony, OpenSSH's client
tools, dosfstools, curl, dig, host and nslookup (`pkg:bind`), openssl,
the CA certificates (`pkg:certs`), and the libraries they load. It also
has three recipes, absent (below): `pkg:claude-code`, `pkg:codex` and
`pkg:grok-build`, each with its `-src`.
A library a present package needs cannot be removed: apply refuses a
broken `needs` graph. The present ones, all but `pkg:oath` (built by
cargo), come from plans in `pkgs/`, built from source; the programs and
libraries with GCC against glibc. Programs load their libraries from
`/lib`, a link farm like `/bin`: a package's `lib/` entries are linked
there when it is present. `base` has no C compiler; `desk` adds
`pkg:gcc`.

**Recipes** (R23, R31): a package whose licence does not allow
redistribution, or a coding agent each machine takes from its vendor's
own build, is not in the image. The catalog has its plan
(`plan:<name>`), its source (`pkg:<name>-src`, the vendor's file by
`url` and `file`) and `pkg:<name>` absent at the pin the build host
got; INDEX lists it under Recipes. Build it here with `sudo oath build
plan:<name>`, then `oath set pkg:<name> present=true` and apply: the
pin refuses a pack that is not the build host's. `pkg:claude-code`
(`claude`), `pkg:codex` and `pkg:grok-build` are recipes.
