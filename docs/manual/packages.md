# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/<hash>/`. `/bin` is a **symlink farm**, not an
unpack target, and so is `/lib`: glibc's loader
(`/lib/ld-linux-x86-64.so.2`) and every linked pack's `lib/` entries
(decision R17). There is no `/lib64` and no RPATH; a program finds its
libraries through `/lib`. Name is a slot; hash is the bits (`sha256-` plus 64 hex
of `oath-tree-v1`). There is no `apt` and no `oath install`.

The slot also has `live` → `<hash>` and compatibility links (`bin` →
`live/bin`, …) so paths like `/oath/store/pkg/<name>/bin` resolve.
Those links are not part of the hash.

## What ships

The `base` profile:

Not removable (`present=false` is refused):

| Id | What |
|----|------|
| `pkg:glibc` | the C library and its loader |
| `pkg:bash` | `bash`, also `/bin/sh` |
| `pkg:coreutils` | `ls`, `cp`, `cat`, `sort`, `hostname`, … |
| `pkg:util-linux` | `mount`, `dmesg`, `sfdisk`, `wipefs`, `blkid`, `lsblk`, `findmnt` |
| `pkg:iproute2` | `ip`, `ss`, `bridge`, `tc` |
| `pkg:dhcpcd` | the DHCP client of `net:net0` |
| `pkg:btrfs` | `btrfs`, `mkfs.btrfs` |
| `pkg:oath` | `oath` |
| `pkg:dropbear` | the SSH server: `dropbear`, `dropbearkey` |

Present and removable: `sed`, `grep`, `gawk`, `findutils`, `diffutils`,
`tar`, `gzip`, `xz`, `bzip2`, `procps` (`ps`, `pidof`, `free`, `top`,
`kill`), `less`, `vim` (also `vi`), `iputils` (`ping`, `tracepath`,
`arping`), `chrony` (`svc:time`), `openssh` (client tools: `ssh`, `scp`,
`sftp`, `ssh-keygen`, `sftp-server`), `dosfstools` (`mkfs.fat`), `curl`
(Mbed TLS, CA bundle at `/oath/store/pkg/curl/live/ssl/cert.pem`),
`bind` (`dig`, `host`, `nslookup`; no `named`), `openssl` (`openssl`;
no CA certificates of its own), and the libraries they load:
`ncurses`, `gmp`, `mpfr`, `zlib`, `zstd`, `lzo`, `mbedtls`, `libuv`,
`liburcu`, `libcap`. A pack's `needs` lists the libraries it loads, so
apply refuses removing `pkg:ncurses` while `pkg:less` is present.

Every pack is built from pinned source on the build host: the Oath
binaries (`pkg:oath`) from this tree, the rest from `pkgs/`
([Build](build.md)). There is no C compiler on the box yet: the
toolchain (`pkg:gcc`) exists on the build host but is not in the base
profile.

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

- `hash` is the pin. Apply hashes the tree again before linking it and
  refuses one that changed on disk. Empty hash is
  discovery (`actual.hash` reports what is live). Switch with
  `oath set pkg:foo hash=sha256-…`, then apply. Other hashes stay on
  disk.
- `url`: if `present` and the store is missing, apply fetches it, then
  links. A `.tar` is a pack tree. An origin prefix plus `hash` fetches
  `{url}/pkg/{name}/{hash}.tar`. A single file becomes `bin/<name>`.
- `file`: with `url`, the URL is one file, a vendor's release, and the
  tree is that file alone at this path, not executable (a recipe's
  source; see Recipes).
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
the pinned needs mounted and no network. Its `/bin` and `/lib` are link
farms of those packs, as on a machine, so a plan pins every pack its
tools load: `apps/hello.plan` pins bash, coreutils, gcc and glibc, and
gmp, mpfr and mpc, which they run with.

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

Known gap: the base image does not carry `pkg:gcc`, so
`apps/hello.plan` builds on the build host (in `.cache/oath`), not on
the box. The plans in `pkgs/` are the larger examples.

## Recipes

A package whose licence does not allow redistribution is a **recipe**
(R23): the image holds its plan, not its pack, and the machine builds
the pack itself from the vendor's own file. `oath` (INDEX.md) lists
the recipes under **Recipes** until their package is present. Claude
Code is one:

```
sudo oath build plan:claude-code         # fetches ~240 MB from downloads.claude.ai
sudo oath set pkg:claude-code present=true
sudo oath apply pkg:claude-code
claude --version                         # 2.1.292 (Claude Code)
```

`claude doctor` then warns that `~/.local/bin/claude` is missing: that
is where the vendor's own installer puts it, and Oath does not use it.

`pkg:claude-code-src` is the vendor's file: `url` is the file itself
and `file` is where it goes in the tree (`src/claude`); `hash` pins
that tree, so a changed download is refused. The plan builds with
packs every machine has (bash, coreutils, grep, glibc) and must make
the very pack the build host made: `pkg:claude-code`'s `hash` is that
pin, and apply refuses another. `claude` runs with `DISABLE_UPDATES=1`;
a newer Claude Code is a new pin from the build host (`cargo make
update` moves the plan and the source; a built `pkg:claude-code` is
left at its version, with the commands to rebuild it). Its settings,
login and history are under `/home/.claude` and `/home/.claude.json`.

## Host store cache and publishing

The build host keeps the same layout under `.cache/oath/store` (or
`$OATH_STORE`):

```
cargo make store --name hello --from apps/hello --tar
cargo make advisories            # OSV lookup from components.json
cargo make publish --dest <dir>  # the profiles' packages and their sources: pkg/, plan/, adv/, INDEX.md
```

`publish` copies only the packages a seed profile names (today the
33 in `base`, besides the recipe), each at the hash this tree records for it: the
product in `pkgs/lock.json`, or `pkg:oath`'s last `cargo make build`.
Each needs a licence on record: `lock.json`'s, or for `pkg:oath` the
workspace licence AND every crate's in its `components.json`. It
refuses, before copying anything, a package that is not built, has no
licence, or needs a package no profile names. Each pack goes out with
its source: the plan that built it (`plan/<name>/<plan hash>.plan`)
and the source pack the plan pins (`pkg/<name>-src/<hash>.tar`); a
pack whose plan is missing or does not match the record is refused.
`INDEX.md` lists name, hash, licence, plan and source. A recipe goes
out as its plan only, listed with the product hash it builds. Build tools
(`gcc`, `rust`, ...), test packs and other hashes in the cache are
never published; the last line names what was left out. A pack made with `cargo make store`
is published only if a profile names it.

Optional backup hooks in a pack (not catalog fields):
`libexec/oath-backup-quiesce` and `libexec/oath-backup-thaw`. See
[Services](services.md).
