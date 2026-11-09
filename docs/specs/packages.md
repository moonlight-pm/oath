# Packages

A package is a catalog object `pkg:<name>` plus one or more trees in a
content-addressed store. The name is a slot; the hash is the bits.
Installing is `oath set pkg:<name> present=true` and `oath apply`.
Every package is native: built from source by an Oath plan, or the
vendor's own release pinned by sha256.

**Status:** partly built. The store, pins, needs graph, descriptions,
components and advisories are built. The native rule holds for the
image: every userspace package in it is built from pinned source by an
Oath plan (`pkgs/`, [builds.md](builds.md)). The publish allowlist and
the pack manifest are built, and so is the store integrity check
(2026-10-16).

Replaces: T11, T12, T18, T20, T32, T41, T42, T44, and the packaging
parts of T24 and T35.

## Store (built)

```text
/oath/store/pkg/<name>/
  <hash>/            one realization: bin/, lib/, share/, INDEX.md, components.json
  live -> <hash>     the realization /bin points at
  bin -> live/bin    convenience links into the live tree
/oath/store/adv/<name>/<hash>.json   advisories for one realization
```

- A realization id is `sha256-` plus 64 hex: SHA-256 of a canonical
  listing of the tree (`oath-tree-v1`). Same bits, same hash. A hash is
  never overwritten with other contents, and never half there: a tree
  is copied beside its slot, hashed, and renamed into place.
- `/bin` is a symlink farm. `/bin/<file>` points at
  `/oath/store/pkg/<name>/<hash>/bin/<file>`. There is no `/usr` and no
  second PATH. Two packages that ship the same `/bin` name refuse. A
  re-pin removes the links of names the new tree no longer has, so a
  program a package drops leaves `/bin`.
- One live realization per name. Two programs runnable at once are two
  names (`pkg:foo`, `pkg:foo-wip`).
- Undo does not roll the store back: it restores a package's spec and
  relinks, and the trees a kept generation pins are still there.

## Integrity (built)

- Before linking a present package, apply hashes its realization again
  and refuses one that no longer hashes to its name ("the store is
  damaged"); actual `verified` (`ok`, `at`, and `got` on a mismatch)
  records the check.
- `oath verify [pkg:…]` hashes every linked tree (or the ones named)
  again. As root it writes `verified` into each actual and regenerates
  INDEX, which lists damaged packages under `## Store damage`; a
  damaged package's status is `error`. It exits 1 if any is damaged.
  Without the lock it only reports.
- PID 1 runs `oath verify --quiet` once per boot, after `ready`, as a
  task: the hashing never delays boot, and its report (damaged packages
  and the count) goes to `init.log`.

## What stays (is, R62)

An Oath machine holds what its packages prescribe, and nothing else. A
version stays while something names it: a pin in the catalog (desired,
applied or live), another package's need, the release the machine runs, a
current plan's build or run pack, or a generation kept for undo. A tree no
one names yet stays for a day, since `oath store` and `oath build` put it
there and its pin comes next. A filed plan stays while a plan object or a
kept generation pins it.

- **Generations.** The newest five stay, as the ESP keeps five boots, and
  so does every one taken since this boot: what was pinned since boot may
  still be running (a seat's service across an update, the kernel's
  modules). So `oath reap --confirm` is safe at any time, and a boot leaves
  exactly what is prescribed. Undo stops at the last one kept, and says so.
- **PID 1** runs `oath --confirm reap --boot` once a boot, as a task beside
  `oath verify`, which also clears an update's or a build's working
  directories that the boot cut off. Under the catalog's lock a tree is
  renamed aside and its status updated, and the space is freed after the
  lock, so an apply never waits on a large delete.
- **`oath reap`** without `--confirm` deletes nothing and takes no lock: per
  version its `size`, `arrived` and `held` (`pin`, `live`, `release`,
  `arrived`, `needs:pkg:<name>`, `plan:<name>`, `generation:<n>`; empty
  means it goes). `--json` is the document a program reads.
- **An update takes what its release dropped.** A package or service the
  release a machine runs had, and the new one lacks, is removed in the
  update's generation, unless `--keep` names it or a package that stays
  needs it. One the owner made was in no release and stays. A machine that
  left a package behind before this rule is cleaned by hand, with `oath rm`.
- Space comes back as snapshots go: `@gen-N` and `@boot-N` are snapshots of
  `@`, so a tree removed from `@` is freed when no snapshot holds it.

## Object (built)

Desired: `present`, `hash` (the pin), `url` (an origin), `needs`,
`description`, `home`, `category`, `source`.

Actual: `present`, `links`, `removable`, live `hash`, other
`realizations` on disk, `components`, `advisories`, plus copies of the
descriptive fields and `source`.

- **Source (is, 2026-10-31, R47).** `source` says where the pinned tree
  was built from: `{repo, commit, branch, dirty, built_by, built_at}`
  (schema `pkg`). The tool that pins a tree records it beside the hash
  (Sola's `cargo make oath apply`); it is on the object, never in the
  tree, so it changes no hash. It is applied like `description`: in a
  generation, put back by undo, and an apply that changes only those
  records fetches, hashes and links nothing. It is not a fetch location
  (R25): it names a commit, not where a pack is hosted.
- **Overlay (is, 2026-10-31, R47).** A present package whose live hash
  is not the pin of the release the machine runs (`/oath/output.json`,
  whose `pins` list every package's) is an overlay: `oath get` and
  `oath diff` name it, with its `source` when it has one, and `oath
  update` replaces it with the release's pin (`--keep` keeps it).

- **Pin.** `desired.hash` is what apply links, and apply verifies the
  tree matches it. There is no global "right" hash for a name; the
  first pin is a trust act by the owner. After that, a mismatch is an
  error, never a silent upgrade.
- **Removable.** The seed profile lists fixed packages (`glibc`,
  `bash`, `coreutils`, `util-linux`, `iproute2`, `dhcpcd`, `btrfs`,
  `oath`, `openssh`, `zsh`), recorded as `fixed` in their
  `meta.json`; `present=false` or `oath rm` on one of them is refused.
  A package the running `oath`'s `base` profile fixes counts as fixed
  without the mark (a machine updated to an image that added it made
  it with `oath new`), and an undo of the apply that made it keeps it.
  Actual `removable` reports it.
- **Needs.** A list of `{id, hash}`: other packages this one needs at
  run time, each at a pin. Apply refuses cycles, refuses `present=true`
  while a need is absent or at another hash, and refuses
  `present=false` while a present package still needs it. Apply never
  rewrites other objects to satisfy the graph.
- **Description, home and category.** One paragraph, a project URL, and
  one word for what the package mostly is (R63; `pkgs/<name>/pkg.toml`, or
  the profile for a pack pinned from elsewhere). Catalog facts, not fetch
  sources; `oath update` writes the release's. Oath's own use `agents`,
  `base`, `browser`, `desk`, `games`, `graphics`, `kernel`, `machines`,
  `network`, `security`, `server`, `sound`, `text`, `toolchain` and `x11`.

## Filling the store (built)

A tree reaches the store one of three ways:

1. Packed into the image by `cargo make build`.
2. Fetched: when the pinned tree is missing and `url` is set, apply
   (or a plan's pre-build) fetches the URL itself when `file` is set
   (one file, placed at that path, not executable: a vendor's release,
   R23), or the URL itself (a `.tar`) or
   `{url}/pkg/<name>/<hash>.tar` from an origin, unpacks it, hashes it,
   and refuses a mismatch. `url` without `hash` fetches and reports the
   hash (discovery, not a pin). With no `url` and a pin, apply and a
   plan's pre-build try each of `host:local.origins` in order (R25,
   built 2026-10-19) until one serves the pinned tree. The local store
   wins when the tree is already there.
3. Built by `oath build` from a plan (see builds.md).

An origin is any HTTPS tree of `pkg/<name>/<hash>.tar`,
`plan/<name>/<hash>.plan`, `adv/<name>/<hash>.json` and `INDEX.md`,
and of the signed releases machines update to (`out/`, R38,
[boot-and-install.md](boot-and-install.md)): a
static bucket, a LAN mirror, or another Oath box. It is not a
repository kind and not canonical. Packs come from anywhere (R25): a
pack is its hash and records no location (its tree, its plan and its
source pack hold no URL), so any origin may serve any pack and the
hash decides. `INDEX.md` lists every pack Oath publishes, and `index.json`
is the same rows with version, category and size for a program to read
(a pack pinned from elsewhere is its owner's, so neither has it). Only
the consumer says where it looks:
`host:local.origins`, in order, or one pkg's own `url`. The machine's
log (`/oath/log/oath.jsonl`, a `pkg` event's `fetched`) records the URL
a tree came from; the pkg object does not.

The default origin (is, 2026-10-19) is `https://store.oath.moonlight.pm`,
first in the `base` profile's `host:local.origins`: the R2 bucket
`oath-store` behind a Cloudflare custom domain, public read; only a
build host's key writes it (buccina's, R38). The product page at `oath.moonlight.pm` is
separate (a Cloudflare Worker, R79).

Host side: `cargo make store --name <n> --from <dir> [--tar]` hashes a
tree into `~/.local/share/oath/store`. `cargo make publish` uploads the
publishable packs, their plans, sources and advisories to the default
origin, then the newest checked output's signed release; `--dest
<dir>` writes the same tree into a directory (below).

## Heads (R72)

**Slices 1 (heads) and 3 (seeds) are built; 2 and 4 will be.** A package
is published on its own, by
whoever makes it, and machines meet only at the origin. Today a pack
reaches a machine through a profile's release (R38): Sola publishes,
Oath pins the hash in `desk`, buccina builds and publishes the whole
output, and each machine updates (a reboot, since the seed moved PID 1).

- **The head.** `pkg/<name>.json` on an origin, beside the packs
  (`pkg/<name>/<hash>.tar`), as `out/<profile>.json` sits beside
  `out/<id>.json`: `{name, hash, needs, committed, published, source,
  version, description, home, category}`, signed beside itself
  (`pkg/<name>.json.sig`, `ssh-keygen -Y sign`, namespace `oath-pkg`).
  `committed` orders heads, as it orders releases (R45).
- **Publishing.** `oath publish pkg:<name>` on any machine puts its
  own pin on the origin: the pack if the origin lacks it, then the
  head, signed by `~/.config/oath/sign_ed25519` and written with the
  origin's write key (`~/.config/oath/origin.env`, as for `cargo make
  publish`). On novus: build, `oath store`, `oath set`, `oath apply`,
  then `oath publish`, and every machine can take it.
- **Trust.** A `host:local.publishers` line may start with
  `names="sola,sola-*"`: that key signs heads for those names only (a
  trailing `*` matches any rest). A key with no list signs any head,
  a release and the audit's report; a scoped key signs only heads.
- **Taking heads.** `oath update` reads the head of every package the
  machine has (and of what those heads need) from its origins, and
  moves each to the newest head a key may sign for it. It refuses a
  head older than the one it took, recorded in `/oath/heads.json`. A
  pin that is not its head is an overlay, replaced unless `--keep`
  (R47). A head wins over a release's pin of the same name, and a
  package with a head is never dropped for leaving a release.
- **Needs.** A head's needs are pins. When the moves leave a need at
  another pin (buccina's mesa moved, Sola's head did not), the update
  re-points it if every `DT_NEEDED` name of the package's ELF files is
  in `/lib` after the moves or in the package's own tree; otherwise
  it holds the dependency at the machine's pin and names the package
  that holds it. `dlopen` is not checked.
- **Services** (slice 2). A tree may ship `oath/svc/<name>.json`, each a
  `svc` spec; making the package present makes each service the
  machine lacks. The owner's changes stay theirs.
- **Seeds** (slice 3, is). `base` and `desk` are data, compiled into
  neither `oath-core` nor PID 1: pkg:oath ships them as
  `/lib/oath/seed/<name>.json`, read at run time (`OATH_SEED_DIR` first;
  `cargo make` and the tests set it to `crates/oath-core/seed`). A pin in
  a profile no longer moves PID 1's hash. PID 1 still seeds a machine
  that has no catalog at all, from those files.
- **Oath's own packages** (slice 4). buccina's publish writes a head for
  every package it builds; the kernel, initrd and PID 1 move as
  `pkg:linux` and `pkg:oath` (the ESP and a PID 1 re-exec, R73), and
  `out/<profile>.json` goes once every machine follows heads.
- **Later, not designed:** an official release, a signed file that pins
  every package, which a machine may follow instead of heads.

## Components and advisories (built)

- `components.json` at the root of a tree lists the upstreams in it:
  `{name, version, purl}` and optional `cpe`, sorted by `purl`, as a
  bare list (what `cargo make pkgs` writes) or under `components`. It is
  part of the tree hash. It is a claim, not a proof. A missing file
  means unknown, not safe.
- There is no `version` field on the slot; a name can hold unrelated
  programs.
- Advisories change after the bits freeze, so they live outside the
  tree at `adv/<name>/<hash>.json`. The build host computes them from
  OSV (`cargo make advisories`). The guest never queries a feed; apply
  fetches the file best-effort from the pkg's `url` (every apply) or
  from the origin that just served the pack. Offline apply still
  works.

- **The audit (is, 2026-10-31, R48).** `cargo make audit` matches
  every package by `pkgs/audit.toml` (CPE, product names, repo and tag,
  OSV name; outside every plan, so no hash moves) against CVE List V5,
  OSV (ecosystem purls, PyPI/Go/Linux names, release-tag commits), NVD
  by CPE, KEV and EPSS. NVD alone sees little since 2026-04-15 (it
  enriches 15–20% of new CVEs), and OSV's purl lookup none of the
  `pkg:generic` upstreams, so CVE List V5's own records carry most of
  it. Their ranges are read as their issuers mean them: the CNA's entry
  outranks CISA's added ones where it holds a range for the version;
  ranges that list only affected versions make the rest unaffected;
  commit ids in "custom" ranges and products with no versions decide
  nothing. The `oath-audit` skill triages and fixes on buccina daily
  (`svc:audit`, `check/audit/`).
- **The report (is, R61).** After each run the runner puts the scan's JSON
  on the origin: `audit/latest.json`, public, with `.sig` beside it, signed
  by the build host's key under the namespace `oath-audit`. `oath audit [pkg:x]`
  reads it from the first origin a key in `host:local.publishers` signed
  and says what it found for the packages here; `--json` is the document:
  `{generated, commit, sources, packages[], origin, signer}`, each package
  `{name, version, hash, upstream?, cpe?, hits[], suppressed, skip?, newer?,
  upstream_unread?}`. Only the JSON goes out; the mailed report stays
  private. A package at another version than the report's was not
  assessed, which is not the same as clean; another build of the same
  version (`hash`) may lack the patches `suppressed` names; a source that
  failed says so in `sources`, and the report then has fewer hits. A pack
  pinned from elsewhere is in it by what its `components.json` names.

## Native packages (policy, D7)

- Every package is either built from source by an Oath plan, or is the
  vendor's own binary release pinned by sha256 (examples: the Rust
  toolchain, linux-firmware, Steam). CEF is built from source with
  Chromium's own Clang and Rust as build inputs (R40).
- A package whose licence does not allow redistribution is a recipe
  (R23, built 2026-10-19): every catalog holds its plan
  (`plan:<name>`) and its source as a pin on the vendor's file
  (`pkg:<name>-src` with `url` and `file`), and `pkg:<name>` absent
  at the product hash the build host got. The machine fetches the file
  and builds the pack with `oath build plan:<name>`, fetching any build
  pack it lacks from its origins (R35); no image or origin carries the
  pack. Claude Code (`pkg:claude-code`) is the first;
  Codex and Grok Build follow it by choice, not licence (R31).
- Never another distribution's packages: no Nix closures, no `.deb`,
  no Arch packages, no third-party rebuilds of an upstream.
- A build host's compiler may bootstrap the first toolchain. It never
  ships.

As built (2026-10-17), `pkgs/lock.json` pins 51 packages, each with its
version, SPDX licence, how it was built, plan hash, product hash and
the packs it runs with, and every one is a plan product: glibc, GCC
and binutils (`pkg:gcc`, with gmp, mpfr and mpc); the GNU userland
(R18: bash, coreutils, sed, grep, gawk, findutils, diffutils, patch,
tar, gzip, xz, bzip2, make, procps-ng, less, vim with ncurses);
util-linux, iproute2, iputils, dhcpcd, chrony, btrfs-progs
(`pkg:btrfs`), OpenSSH (sshd, R36, with libxcrypt for `crypt(3)`;
client, scp, sftp, ssh-keygen, sftp-server; with OpenSSL), dosfstools, curl (with the CA bundle at
`/oath/store/pkg/curl/live/ssl/cert.pem`), BIND 9's dig, host and
nslookup (`pkg:bind`, 2026-10-19), OpenSSL (its `ssl/cert.pem` links
to curl's bundle) and the libraries they load
(zlib, zstd, mbedTLS, libuv, liburcu, libcap); the kernel (`pkg:linux`) and firmware
(individual linux-firmware blobs); kexec-tools (`pkg:kexec`, static)
for the installer; and the build tools (`pkg:rust`, `pkg:go`, python, meson,
ninja, perl, m4, flex, bison, bc, elfutils, pkgconf, patchelf).
`pkg:certs`, Oath's CA roots (R50), is in `base`, from the release after 2026-11-09 (R77).
In no profile: servers built from
source (R49): Blocky, restic, rsync, `route`, `cell` with `crun` and
`passt`, PostgreSQL and PostGIS with theirs.
`pkg:oath` follows this tree and is not locked. Nothing in the image
comes from nixpkgs.

## Shared libraries (built, R17)

Programs link dynamically against glibc, whose loader is
`/lib/ld-linux-x86-64.so.2`. `/lib` is a link farm of every linked
package's `lib/` entries (not `oath/`, `modules/`, `firmware/` or
`pkgconfig/`), made and removed with its `/bin` links, so a re-pin of
a library changes the programs that load it, as on any distro; sonames
keep incompatible versions apart, the `needs` graph checks the set,
and undo takes a bad re-pin back. No `/lib64`, and Oath sets no
RPATHs; libtool builds keep one naming their own slot's `live/lib`
(curl, BIND), which `/lib` links anyway. The image
writes each pack's `needs` from the lock's run packs (the libraries it
loads), so apply refuses removing `pkg:ncurses` while `pkg:less` is
present.

## Notes, lineage and vendors (will be, not built)

Joshua (2026-10-31), for when Oath's packages reach people beyond us;
recorded, not designed or built. It is Open in
[decisions.md](../decisions.md).

**The problem.** A person choosing a package has a name and a hash. The
hash says exactly which bits; the name is a slot. Neither says what
changed, whose package it is, or how it relates to another one:

1. **Notes per version.** A person reads what a version is before
   choosing it, as a `CHANGES.md` per release, and two versions of one
   package (two hashes offered for one name) must read apart.
2. **A chain.** A package's heritage: what it was built from, the line
   of versions before it, and whose line it is.
3. **Vendors.** Oath is decentralized (R25: a pack records no location,
   any origin may serve it). Different vendors can offer packages of the
   same name, so `pkg:sola` from two origins may be two unrelated lines,
   and nothing today tells them apart or says which one a machine
   follows.

**What there is today, and how it relates.**

- *The content hash* is the identity of the bits and stays so. Notes and
  chain are about a hash, not in its tree: in the tree, every note would
  move the hash (the same reason R47 keeps `source` out of it).
- *`components.json`* (in the tree, hashed) names the upstreams and
  their versions: which upstream release, not which vendor's build or
  what changed.
- *The plan and source pack* (on the origin, by hash, R13) say how a
  tree was built, reproducibly: the "built from" half of a chain,
  already content-addressed, with no "what came before".
- *`pkg.source`* (R47) is the machine's record of the commit, branch
  and builder of the tree it pinned. It is local, written by whoever
  pinned it, unsigned, and does not travel with the pack: it answers
  "what is live here, from where", not "what changed" or "whose line".
  A chain entry would carry the same facts, signed by the vendor, with
  `source` its copy on the machine.
- *Advisories* (`adv/<name>/<hash>.json`) are the model for metadata
  that comes after the bits: a sidecar keyed by name and hash, served
  beside the pack. Notes and a chain entry could be one too (say
  `meta/<name>/<hash>.json`: version, notes, the previous hash in its
  line, the vendor), signed by the vendor's key.
- *Releases* are signed by a build host's key that machines trust by
  hand (`host:local.publishers`, R38); packs are not (below). A
  vendor's identity would be a key too, not a name: a line is a key and
  a name, and each entry names its predecessor's hash under the same
  key, so a chain cannot be spliced.

**Open in it.** The notes' format; who signs and how a machine trusts a
vendor's key; whether names carry the vendor (a naming decision, as
kinds are); where a machine records the line it follows; how `oath
update` moves within a line and never across one silently; and how it
meets the unsigned packs below.

## Target

- Out of scope for now: signatures on Oath's own packs and indexes
  (upstream sources are signature-checked at build time, see
  builds.md), peer discovery.

## Publish allowlist (built)

`cargo make publish` puts on an origin every pack `pkgs/lock.json`
records, build packs included (`pkg:gcc`, `pkg:rust`, `pkg:python`, …,
R25), and every package a seed profile names (present or not), at the
hash the tree records (`pkgs/lock.json`, or `pkg:oath`'s last build),
each with a licence on record: the SPDX expression in `lock.json`, or
for `pkg:oath` the workspace licence AND every crate's from its
`components.json`. Beside each pack it puts what built it (R13): the
plan, at `plan/<name>/<plan hash>.plan`, and the source pack that plan
pins, at `pkg/<name>-src/<hash>.tar` (the verbatim upstream files,
patches and `build.sh` inputs). It refuses the whole publish if one is
not built, has no licence, has no plan whose hash matches the record,
has a `run` need that is not published, or has a plan whose build packs
are not published at the hashes it pins; so an origin alone rebuilds
every plan it carries. Recipes go out as their plan alone (R23).
`INDEX.md` lists name, hash, licence, plan and source; it goes up last.
To the default origin, publish lists the bucket and uploads only the
hashed files it lacks (`pkg/`, `plan/` never change), and `adv/` and
`INDEX.md` every time; then it reads `INDEX.md` back from the public
URL.

## Pack manifest (built)

`cargo make build` writes `manifest.json` into its output
(`build/out/<id>/`; the id is the manifest's hash): every pack the
image ships (the profile's store, the initrd, the installer's tools,
the ESP), each with its hash, version, licence, plan hash, where it
ships, and its source pins with signing keys, Sigstore identities or
unsigned reasons; `pkg:oath` with the commit it was built from.

## Pack layout

A pack is a directory: `bin/` (each file becomes `/bin/<name>`),
optional `lib/`, `share/`, `libexec/`, an optional short `INDEX.md`
that tells two hashes of one name apart, and `components.json`. How
the tree was produced is the author's business; Oath hashes what is
there.
