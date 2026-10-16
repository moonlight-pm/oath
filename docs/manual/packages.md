# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/<hash>/`. `/bin` is a **symlink farm**, not an
unpack target. Name is a slot; hash is the bits (`sha256-` plus 64 hex
of `oath-tree-v1`). There is no `apt` and no `oath install`.

The slot also has `live` → `<hash>` and compatibility links (`bin` →
`live/bin`, …) so paths like `/oath/store/pkg/<name>/bin` resolve.
Those links are not part of the hash.

## What ships

The `base` profile:

| Id | Default | Removable |
|----|---------|-----------|
| `pkg:busybox` | present | no — all applets, including `/bin/vi`, `/bin/ip`, `/bin/udhcpc` |
| `pkg:btrfs` | present | no — `/bin/btrfs` |
| `pkg:oath` | present | no — `/bin/oath` |
| `pkg:dropbear` | present | no — the SSH server: `dropbear`, `dropbearkey` |
| `pkg:openssh` | present | yes — OpenSSH client tools: `ssh`, `scp`, `sftp`, `ssh-keygen`, `sftp-server` |
| `pkg:util-linux` | present | yes — `sfdisk`, `wipefs`, `blkid`, `lsblk` |
| `pkg:dosfstools` | present | yes — `mkfs.fat`, `mkfs.vfat`, `fsck.fat` |
| `pkg:curl` | present | yes — static curl (mbedTLS) with a CA bundle at `/oath/store/pkg/curl/live/ssl/cert.pem` |

Every pack above is built from pinned source on the build host: the
Oath binaries (`pkg:oath`) from this tree, the rest from `pkgs/`
([Build](build.md)). There is no C compiler on the
box yet: the toolchain (`pkg:llvm`) exists on the build host but is not
in the base profile. There is no glibc on Oath.

PID 1 stays at `/lib/oath/init`; it is not a package.

## Install / remove

```
oath ls --kind pkg
oath get pkg:dosfstools
oath set pkg:dosfstools present=false
oath apply
ls /oath/store/pkg/dosfstools     # the tree stays

oath set pkg:dosfstools present=true
oath apply
readlink /bin/mkfs.fat
oath undo
```

`present=false` removes **this object's** `/bin` links. The store tree
stays, so a re-install needs no network. Apply refuses to clobber a
`/bin` name it does not own. `present=false` on a non-removable
package is refused (not `--confirm`).

## Fields in practice

- `hash` is the pin. Apply verifies the tree matches. Empty hash is
  discovery (`actual.hash` reports what is live). Switch with
  `oath set pkg:foo hash=sha256-…`, then apply. Other hashes stay on
  disk.
- `url`: if `present` and the store is missing, apply fetches it, then
  links. A `.tar` is a pack tree. An origin prefix plus `hash` fetches
  `{url}/pkg/{name}/{hash}.tar`. A single file becomes `bin/<name>`.
- `needs` lists other packs this one needs at runtime, each pinned by
  hash. Apply refuses a broken graph: a missing need, a need at the
  wrong hash, a cycle, or removing a need while a present pack still
  lists it. Apply does not change the other objects for you.
- `description` and `home` are prose and a project URL.

Upstream identity is `components.json` at the pack root (`name`,
`version`, `purl`); it is part of the tree hash, and missing means
unknown. Advisories for a pin live at
`/oath/store/adv/<name>/<hash>.json`, fetched from the origin when
`url` is one. `oath get` shows both in actual. An empty list is not a
safety proof.

## Plans

A **plan** (`plan:<name>`) is a hashed `plan.plan` file that builds a
product tree for `pkg:<name>`: a JSON head (`name`, `produces`,
`build_needs`, `run_needs`, each need pinned by hash) and a `/bin/sh`
script that writes `/out`. `oath build` runs it in a sandbox with only
the pinned needs mounted and no network. It does not link `/bin`.

```
oath fmt -w apps/hello.plan
oath build --file apps/hello.plan
oath set plan:hello --from-file apps/hello.plan
oath build plan:hello
oath set pkg:hello present=true
oath apply
```

A missing `build_needs` pack is fetched first when its `pkg` object
has a `url`. The second build of an unchanged input set is a no-op.
The script's output goes to `/oath/log/plan/<name>.log`, not the
terminal: `oath build` prints the path (and a failed build the last
lines), and `oath logs plan:hello` reads it. The previous build's log
is `hello.log.1`.

The build cache is keyed on the plan hash and the input set, so an
edited script always rebuilds.

Known gap: the base image does not carry `pkg:llvm`, so
`apps/hello.plan` builds on the build host (in `.cache/oath`), not on
the box. The plans in `pkgs/` are the larger examples.

## Host store cache and publishing

The build host keeps the same layout under `.cache/oath/store` (or
`$OATH_STORE`):

```
cargo make store --name hello --from apps/hello --tar
cargo make advisories            # OSV lookup from components.json
cargo make publish --dest <dir>  # the profiles' packages: pkg/, adv/, INDEX.md
```

`publish` copies only the packages a seed profile names (today the
eight in `base`), each at the hash this tree records for it: the
product in `pkgs/lock.json`, or `pkg:oath`'s last `cargo make build`.
Each needs a licence on record: `lock.json`'s, or for `pkg:oath` the
workspace licence AND every crate's in its `components.json`. It
refuses, before copying anything, a package that is not built, has no
licence, or needs a package no profile names. `INDEX.md` lists name,
hash and licence. Source packs, build tools (`llvm`, `rust`, ...),
test packs and other hashes in the cache are never published; the
last line names what was left out. A pack made with `cargo make store`
is published only if a profile names it.

Optional backup hooks in a pack (not catalog fields):
`libexec/oath-backup-quiesce` and `libexec/oath-backup-thaw`. See
[Services](services.md).
