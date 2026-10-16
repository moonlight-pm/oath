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
the pack manifest are built.

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
  second PATH. Two packages that ship the same `/bin` name refuse.
- One live realization per name. Other hashes of the same name may stay
  on disk. Two programs runnable at once are two names
  (`pkg:foo`, `pkg:foo-wip`).

## Object (built)

Desired: `present`, `hash` (the pin), `url` (an origin), `needs`,
`description`, `home`.

Actual: `present`, `links`, `removable`, live `hash`, other
`realizations` on disk, `components`, `advisories`, plus copies of the
descriptive fields.

- **Pin.** `desired.hash` is what apply links, and apply verifies the
  tree matches it. There is no global "right" hash for a name; the
  first pin is a trust act by the owner. After that, a mismatch is an
  error, never a silent upgrade.
- **Removable.** Engine-owned. The seed profile lists fixed packages
  (`busybox`, `btrfs`, `oath`, `dropbear`); `present=false` on one of
  them is refused.
- **Needs.** A list of `{id, hash}`: other packages this one needs at
  run time, each at a pin. Apply refuses cycles, refuses `present=true`
  while a need is absent or at another hash, and refuses
  `present=false` while a present package still needs it. Apply never
  rewrites other objects to satisfy the graph.
- **Description and home.** One paragraph and a project URL. Catalog
  facts, not fetch sources.

## Filling the store (built)

A tree reaches the store one of three ways:

1. Packed into the image by `cargo make build`.
2. Fetched: when `url` is set and the pinned tree is missing, apply
   fetches either the URL itself (a `.tar`) or
   `{url}/pkg/<name>/<hash>.tar` from an origin, unpacks it, hashes it,
   and refuses a mismatch. `url` without `hash` fetches and reports the
   hash (discovery, not a pin). The local store wins when the tree is
   already there.
3. Built by `oath build` from a plan (see builds.md).

An origin is any HTTPS tree of `pkg/<name>/<hash>.tar` and
`adv/<name>/<hash>.json`: a static bucket, or another Oath box. It is
not a repository kind and not canonical.

Host side: `cargo make store --name <n> --from <dir> [--tar]` hashes a
tree into `.cache/oath/store`. `cargo make publish --dest <dir>` copies
the publishable packs and their advisories into an origin tree (below).

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
  fetches the file best-effort from the origin. Offline apply still
  works.

## Native packages (policy, D7)

- Every package is either built from source by an Oath plan, or is the
  vendor's own binary release pinned by sha256 (examples: the Rust
  toolchain, linux-firmware, Steam, CEF).
- Never another distribution's packages: no Nix closures, no `.deb`,
  no Arch packages, no third-party rebuilds of an upstream.
- A build host's compiler may bootstrap the first toolchain. It never
  ships.

As built (2026-10-16), `pkgs/lock.json` pins 27 packages, each with its
version, SPDX licence, how it was built, plan hash and product hash,
and every one is a plan product: the image's busybox, btrfs-progs
(`pkg:btrfs`), dropbear, OpenSSH (client, scp, sftp, ssh-keygen,
sftp-server; no OpenSSL), util-linux, dosfstools and curl (with the CA
bundle at `/oath/store/pkg/curl/live/ssl/cert.pem`); the kernel
(`pkg:linux`) and firmware (individual linux-firmware blobs);
kexec-tools (`pkg:kexec`) for the installer; and the build tools
(`pkg:llvm`, `pkg:rust`, make, cmake, ninja, python, perl, m4, flex,
bison, elfutils, pkgconf, patchelf and the libraries zlib, zstd, lzo
and mbedTLS). `pkg:oath` follows this tree and is not locked. Nothing
in the image comes from nixpkgs.

## Target

- **Integrity.** Apply re-verifies the live tree hash before linking,
  and the store is never rolled back by undo.
- **RPATHs** inside packs point at hash paths, not at `live`, so a
  re-pin cannot change a running program's libraries.
- Out of scope for now: signatures on Oath's own packs and indexes
  (upstream sources are signature-checked at build time, see
  builds.md), reaping unused realizations, peer discovery.

## Publish allowlist (built)

`cargo make publish` puts on an origin only the packages a seed profile
names (present or not), at the hash the tree records (`pkgs/lock.json`,
or `pkg:oath`'s last build), each with a licence on record: the SPDX
expression in `lock.json`, or for `pkg:oath` the workspace licence AND
every crate's from its `components.json`. Beside each pack it puts
what built it (R13): the plan, at `plan/<name>/<plan hash>.plan`, and
the source pack that plan pins, at `pkg/<name>-src/<hash>.tar` (the
verbatim upstream files, patches and `build.sh` inputs). It refuses the
whole publish if one is not built, has no licence, has no plan whose
hash matches the record, or has a `run` need that no profile names.
`INDEX.md` lists name, hash, licence, plan and source. The build packs
a plan pins (`pkg:llvm`, `pkg:make`, `pkg:rust`, ...) are not
published.

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
