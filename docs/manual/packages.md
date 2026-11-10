# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/<hash>/`. `/bin` is a **symlink farm**, not an
unpack target, and so is `/lib`: glibc's loader
(`/lib/ld-linux-x86-64.so.2`) and every linked pack's `lib/` entries
(decision R17). There is no `/lib64`. A program finds its libraries through
`/lib`; some packs also carry an RPATH or RUNPATH (`curl`'s is
`/oath/store/pkg/curl/live/lib`, `rustc`'s is `$ORIGIN/../lib`). Name is a slot;
hash is the bits (`sha256-` plus 64 hex of `oath-tree-v1`). There is no `apt`.
`oath install` takes no package: it puts the system on a disk
([Install](install.md)). A package comes by name with `oath add`
([Install / remove](#install--remove)), through `oath new` or `oath set` then
`oath apply`, or with `oath update`.

The slot also has `live` → `<hash>` and compatibility links (`bin` →
`live/bin`, …) so paths like `/oath/store/pkg/<name>/bin` resolve.
Those links are not part of the hash.

There is no `/usr`: a script that starts `#!/usr/bin/env` does not run
(`bad interpreter`), and one that starts `#!/bin/env` does. `/var` holds
`empty`, `lib` and `run`; there is no `/var/tmp` or `/var/log`.

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
| `pkg:btrfs` | `btrfs`, `mkfs.btrfs`. Built without LZO: `btrfs restore` cannot read LZO extents; the kernel can |
| `pkg:oath` | `oath` |
| `pkg:openssh` | the SSH server `sshd` (R36), and `ssh`, `scp`, `sftp`, `ssh-keygen`, `sftp-server` |
| `pkg:zsh` | `zsh`, `home`'s login shell |

What the running `oath`'s base profile fixes is what counts: a package it
does not fix can be removed, even one a machine was seeded with as fixed.

Present, and removable once nothing present needs them: `sed`, `grep`, `gawk`,
`findutils`, `diffutils`, `tar`, `gzip`, `bzip2`, `procps` (`ps`, `pidof`,
`free`, `top`, `kill`), `less`, `vim` (also `vi`), `iputils` (`ping`,
`tracepath`, `arping`), `chrony` (`svc:time`), `dosfstools` (`mkfs.fat`), `curl`
(Mbed TLS, CA bundle at `/oath/store/pkg/curl/live/ssl/cert.pem`),
`bind` (`dig`, `host`, `nslookup`; no `named`), `certs` (Oath's CA roots, R50),
and the libraries only they load: `mpfr`, `mbedtls`, `libuv`, `liburcu`, `libcap`.

Present and held by a non-removable package, so apply refuses removing them:
`gmp` (`coreutils`), `ncurses` (`zsh`), `zlib` (`btrfs`, `openssh`), `zstd` (`btrfs`),
`xz` (through `zstd`), `libxcrypt` and `openssl` (`openssh`). `openssl` is also the
`openssl` tool; its default `ssl/cert.pem` links to curl's bundle.

A pack's `needs` lists the libraries it loads, so apply refuses removing
`pkg:mbedtls` while `pkg:curl` is present.

Also in the catalog, with `present=false`: the recipes `pkg:claude-code`,
`pkg:codex` and `pkg:grok-build` and their `-src` packs. Their plans are there
too; a machine builds each one itself (see Recipes).

The `desk` profile extends `base` with the packages below and the libraries and
data they load; `oath ls --kind pkg` lists every one on a machine.

| Packages | What |
|----------|------|
| `gcc`, `make`, `m4`, `bison`, `cmake`, `meson`, `ninja`, `pkgconf`, `perl`, `python`, `rust`, `git` | a compiler and build tools: `cc`, `make`, `cmake`, `cargo`, `python3`, `git`, … |
| `tmux`, `wl-clipboard`, `vulkan-tools`, `bubblewrap` | `tmux`, `wl-copy`, `wl-paste`, `vulkaninfo`, `bwrap` |
| `rivulet`, `wlroots`, `seatd`, `xwayland`, `xkbcomp`, `mesa`, `libglvnd`, `vulkan-loader` | the compositor, the GPU, X clients |
| `pipewire`, `wireplumber`, `libpulse`, `alsa-lib` | sound: `wpctl`, `pactl`, the `pw-*` tools |
| `bluez`, `dbus` | `bluetoothctl`, `btmon`; D-Bus |
| `cef` | the browser engine |
| `font-noto`, `font-noto-cjk`, `font-noto-emoji`, `font-iosevka-term-slab` | typefaces |
| `sola`, `sola-*` | Sola's packs, from Sola's own repository, pinned by hash |
| `steam`, `nvidia` | recipes, `present=false` ([Recipes](#recipes)) |
| `*-i386` | 32-bit libraries for Steam's games |

Every pack but Sola's is made on the build host from pinned sources: the Oath
binaries (`pkg:oath`) from this tree, the rest from `pkgs/`, compiled or, as
with `pkg:rust`, taken from the vendor's own release ([Build](build.md)). The
`base` profile has no C compiler. The `desk` profile has `pkg:gcc`; on a `base`
machine `sudo oath add gcc` takes it, with `pkg:mpc`, which it loads
([Install / remove](#install--remove)).

PID 1 stays at `/lib/oath/init`; it is not a package.

## Install / remove

```
oath ls --origin                  # what the origins hold; * on what this machine has
oath ls --origin sql              # a word in a name, category or description
oath add aws                      # without root: what it would take, and each pack's size
sudo oath add aws                 # at its head, with what it needs that the machine lacks
```

`oath ls --origin [word]` lists the origins' `index.json` ([Origins](#origins)): each
package's name, version, category and what it is, `*` on one this machine has, and a
recipe as ``(a recipe: `oath build` makes it here)``. `oath --json ls --origin` gives
each row's `name`, `version`, `category`, `description`, `license`, `hash` (a recipe's
is the pack it builds), `recipe` and `installed`, the pin here or `null`. The list is
not signed, and nothing installs from it.

`sudo oath add <name>...` takes each package at the newest head a key in
`host:local.publishers` may sign for that name ([Publishing a package on its
own](#publishing-a-package-on-its-own-r72)), with each package it needs that the
machine lacks, each at its own head, and applies them in one generation. It prints
each with its version and signer, then `added in generation <n>`; `oath undo` takes
them back. Nothing the machine has moves. A package already here is named and left to
`oath update`, and one here with `present=false` is refused. A need the machine holds
at another pin is re-pointed when the new package loses no library it loads
(`runs with this machine's pkg:<name> sha256-… -> sha256-…`); otherwise nothing is
made, and `sudo oath update --confirm` comes first. Two packages that would link one name
in `/bin` or `/lib` are refused (`two packages would ship one name: /bin/<x> (pkg:<a> and
pkg:<b>)`) before anything changes, and an apply that stops part way is undone whole
(`undone: back to generation <n>`). A name no origin has a head of is refused
(`pkg:<name> has no head on any origin`): a recipe has none ([Recipes](#recipes)), and
neither do `linux`, `oath` and `firmware`, which move with a release.
`sudo oath new pkg:<name>` with no fields makes the same objects without applying them
and prints the `oath apply` that starts them. For a name no origin has a head of, with one
tree of it in the store (`oath store`), it makes the object with no `hash`, and apply
links that tree. `oath add` records each head it took in `/oath/heads.json`, as an update does.

Run by a user who cannot change the catalog, `oath add` fetches no pack and changes
nothing: it prints `would add` and each package with its pack's size on its origin (or
`in the store already`), and the total to fetch. `oath --json add` gives `would_add`,
each row with `asked` (named, not a need), `size` (bytes, or `null`) and `in_store`;
`size`, the total; `size_known`; and `run`, the `sudo` command that adds them. Until the
packs are here the library check is not run, so `sudo oath add` can still refuse.

```
oath ls --kind pkg
oath get pkg:dosfstools
sudo oath set pkg:dosfstools present=false
sudo oath apply
ls /oath/store/pkg/dosfstools     # the tree stays

sudo oath set pkg:dosfstools present=true
sudo oath apply
readlink /bin/mkfs.fat
sudo oath undo
```

`present=false` removes **this object's** `/bin` and `/lib` links. The store tree
stays, so a re-install needs no network. A new `hash` relinks, and a
name the new tree no longer has leaves `/bin` or `/lib`. Apply refuses to clobber a
`/bin` or `/lib` name it does not own. `present=false` on a non-removable
package is refused (not `--confirm`), and so is `oath rm`. `oath get` shows
`removable` under actual.

`sudo oath rm pkg:foo` marks the object for removal. `sudo oath apply` unlinks
it and deletes the object, and `sudo oath undo` brings it back. Its store tree
stays until `oath reap` finds nothing holding it.

## Fields in practice

- `hash` is the pin. Apply hashes the tree again before linking it and
  refuses one that changed on disk. Empty hash is
  discovery (`actual.hash` reports what is live). Switch with
  `oath set pkg:foo hash=sha256-…`, then apply. Other hashes stay on
  disk while a pin, a need, the release, a current plan or a kept
  generation holds them, and for a day after they arrive; `oath reap`,
  which PID 1 runs once a boot, removes the rest.
- `url`: if `present` and the store is missing, apply fetches it, then
  links. A `.tar`, `.tar.gz` or `.tgz` is a pack tree. An origin prefix plus `hash` fetches
  `{url}/pkg/{name}/{hash}.tar`. A single file becomes `bin/<name>`.
  With no `url`, a pinned pack the store lacks comes from the machine's
  origins (below).
- `file`: with `url`, the URL is one file, a vendor's release, and the
  tree is that file alone at this path, not executable (a recipe's
  source; see Recipes).
- `needs` lists other packs this one needs at runtime, each pinned by
  hash. Apply refuses a broken graph: a missing need, a need at the
  wrong hash, a cycle, or removing a need while a present pack still
  lists it. Apply does not change the other objects for you.
- `description` and `home` are prose and a project URL. `category` is
  one word for what the package mostly is; Oath's own use `agents`,
  `base`, `browser`, `desk`, `games`, `graphics`, `kernel`, `machines`,
  `network`, `security`, `server`, `sound`, `text`, `toolchain` and
  `x11`. None of the three is in a hash, and `oath update` writes the
  release's.
- `source` records where the pinned tree was built from: `{repo,
  commit, branch, dirty, built_by, built_at}` (`oath schema pkg`). Set
  it with the hash it describes, in one apply. It is in no hash, and an
  apply that changes only it (or the prose) touches no store, link or
  service.

Actual has what the machine sees: `links` (its names in `/bin`),
`removable`, the live `hash`, `components`, `advisories` (absent when
never fetched), `realizations` (the trees of this name on disk, each
with `hash`, `linked`, `bins` and `components`; `oath reap` says what
holds each), `verified` (`ok`, `at`, and `got` when the tree hashes to
something else: [A damaged package](#a-damaged-package)), and the applied
`url`, `needs`, `description`, `home`, `category` and `source`.

## A damaged package

A tree that no longer hashes to its pin is damaged. PID 1 hashes every
linked package and cell tree again once a boot, after the services start
(`oath verify --quiet`; `oath logs` shows a line like
`39 verified, 0 damaged`). `oath verify` prints `pkg:<name> DAMAGED: …` and
exits 1. `oath get pkg:<name> --actual` has the outcome in `verified`: `at`,
`ok`, and `got` for a tree that differs. A damaged tree has status `error`,
`oath` (INDEX.md) lists it under **Store damage**, and `oath apply` refuses
to link it. Apply, `sudo oath verify` and the boot's check write
`verified`; `oath verify` as `home` only reports. No check repairs
anything. An apply with no change says `in-sync`, and one that changes
only `description`, `home`, `category` or `source` records it; neither
looks at the tree. To take the tree again from the origins, delete it
and set its `url` to an origin, so apply fetches it:

```
sudo rm -rf /oath/store/pkg/less/sha256-…
sudo oath set pkg:less url=https://store.oath.moonlight.pm
sudo oath apply pkg:less
```

The fetch runs `curl` and `tar`, so `pkg:curl` and `pkg:tar` cannot come
back this way.

## What stays in the store

A tree stays while something holds it, and PID 1 runs `oath reap` once a
boot and removes the rest. `oath reap` says what this machine holds
beyond what its packages prescribe: the versions of each package and cell
that would go and their size, store slots with no object, filed plans
nothing pins, and generations ([Generations](generations.md)).
`sudo oath reap --confirm` removes them at once. `oath undo` cannot reach a
generation that went. A build underway stops `--confirm`, and the preview
says so.

`oath --json reap` lists every version with what holds it (`held`); one
with nothing there goes.

| `held` | The version stays because |
|--------|---------------------------|
| `pin` | a `pkg` or `cell` object's spec, or its actual, names it |
| `live` | the slot's `live` link names it, and the slot has an object |
| `needs:pkg:<name>` | that package needs it at this hash |
| `release` | the release this machine runs pins it |
| `plan:<name>` | that pinned plan builds or runs with it |
| `generation:<n>` | that kept generation pins or needs it |
| `arrived` | it reached the store less than `--arriving` seconds ago (default 86400), so its pin may still come; an `oath store` nobody pins goes at the first boot after that |

## A tree built here

`oath store` puts a tree built on the machine (a directory, or a `.tar`
or `.tar.gz` of one) into the store and prints its hash. It changes no
object and no link; pinning the hash and applying does:

```
sudo oath store pkg:hello ./out            # pkg:hello sha256-…
sudo oath --json store pkg:hello out.tar   # {"hash":"sha256-…","id":"pkg:hello"}
sudo oath set pkg:hello hash=sha256-… needs='[{"id":"pkg:glibc","hash":"sha256-…"}]'
sudo oath apply pkg:hello
```

A tool that pins a tree it built records where from, with the hash:

```
sudo oath set pkg:hello --from-json '{"hash":"sha256-…","source":{"repo":"https://github.com/me/hello","commit":"<40 hex>","branch":"next","dirty":false,"built_by":"home@canto","built_at":"2026-11-01T12:00:00Z"}}'
```

A package's pin is an overlay when it is not the head this machine took of
that package (below) or, with none taken, not the pin of the release it runs.
A name the release does not pin, with no head taken, is no overlay: `pkg:hello`
above is not one. `oath get pkg:less` ends with `overlay: pkg:less is
next@1a2b3c4d, not the release's sha256-… (output …); oath update replaces it`
(or `not its head sha256-… (signed by …)`), and `oath diff` lists every overlay
above its `in-sync`. `sudo oath update --confirm` replaces an overlay of a head
and, when it moves the machine to another release or the package has a head, an
overlay of the release's pin; at the newest release, with no head, that one
stays. `--keep pkg:less` keeps one.

A new name is `oath new pkg:hello hash=…` instead of `set`; a source
pack a plan builds with is `oath new pkg:hello-src hash=… present=false`
and goes in the plan's `build_needs`. Every file is stored 0755 or 0644
(the hash records only whether it is executable), and a pipe, socket or
device in the tree is refused, as a fetch refuses it. A package that is
present needs a `bin/` directory in its tree, even an empty one (`pkg:certs`
has one); without it apply stops at `no store for pkg:<name>`.

A service follows its package: when an apply moves the pin of the
package a service's program comes from (`exec[0]`, through `/bin`),
PID 1 restarts that service, and no other (an update leaves the
desk's running: [Services](services.md)). So a change to a program is
build, `oath store`, `oath set … hash=`, `oath apply`.

## Publishing a package on its own (R72)

Whoever makes a package can publish it from the machine it was built
on. Every machine that trusts that key for the name takes it at its next
`oath update`, and `sudo oath add hello` takes it on one that lacks it: no
profile, release or build host in between. Only `cargo make publish` writes
`index.json`, so `oath ls --origin` does not list a package published only
this way. After the four steps above:

```
oath publish pkg:hello                      # what it would publish, and the head it replaces
oath publish --confirm pkg:hello            # the pack (if the origin lacks it), then its head
oath publish --confirm pkg:hello --dest d   # or write the same files into a directory
oath publish --confirm pkg:hello --committed 2026-11-06T12:00:00Z
```

The head is `pkg/hello.json` on the origin, beside `pkg/hello/<hash>.tar`:
the pin, its needs, `source`, version and description, signed beside
itself (`.sig`, namespace `oath-pkg`) by `~/.config/oath/sign_ed25519`
(`OATH_SIGN_KEY`) and written with the origin's write key, as for `cargo
make publish` (below). The pin must be applied. A pin with `source.dirty`
true is refused; one with no `source` goes out without one. With `--committed` (the commit's time, any
offset; the head keeps it in UTC) it refuses to replace a head built
from a later commit. Publishing needs no
`sudo`, so it cannot write this machine's record of the heads it took
(below): when this machine trusts the key for that name, a confirmed
publish names the step that does, and until it runs `oath get` calls the
published pin an overlay:

```
sudo oath update --record --confirm         # record each head this machine runs; nothing moves
oath update --record                        # or list each package's head, and whether it runs it
```

A machine trusts a publisher for some names only by putting a list
before its key in `host:local.publishers`:

```
sudo oath set host:local publishers='["ssh-ed25519 AAAA… oath@buccina", "names=\"sola,sola-*\" ssh-ed25519 AAAA… oath@novus"]'
sudo oath apply host:local
```

Such a key signs heads of those names only: never a release or the
audit's report, so it cannot ship PID 1 or the kernel. A list that covers
`openssh` (`names="openssh"`, or `names="*"`) lets it sign `pkg:openssh`'s head,
and so sshd. A key with no list signs heads, releases and the audit's report.
Each line is an `ssh-ed25519` public key; a person adds one, an agent does not.

`oath update` reads the head of every package the machine has (and of what
those heads need) from each origin and moves each package to the newest
head a key may sign for it; `oath update` alone says what would move.
It records each head it took in `/oath/heads.json` and refuses an older
one after; `oath add` and `oath update --record` record theirs the same
way, and the record never goes back to an older head. A head wins over a release's pin of the same package, so
`oath update --output <older>` leaves each package with a head at its head
(`oath undo` puts back what an update moved), and a package with a head
stays when a release drops it. With
`host:local.profile` empty a machine takes heads only.

A need is a pin. When the moves leave one at another pin (a new
`pkg:mesa` under a package built against the old one), the update
re-points it if every library the package's ELF files load is still in
`/lib` after the moves, or in its own tree; otherwise it holds the moved
package back and says which package and library hold it:

```
pkg:mesa stays at sha256-…; sha256-… waits: pkg:sola needs pkg:mesa at sha256-…, and would lose libEGL.so.1
pkg:sola keeps its pin; its needs move: pkg:mesa sha256-… -> sha256-… (every library it loads is still in /lib)
```

A preview, whose packs are not fetched yet, says it checks the
libraries once they are; `--confirm` fetches them and checks before
anything changes. A library loaded with `dlopen` is not seen.

Upstream identity is `components.json` at the pack root (`name`,
`version`, `purl`); it is part of the tree hash, and missing means
unknown. Advisories for a pin live at
`/oath/store/adv/<name>/<hash>.json`, fetched from the origin when
`url` is one, or from the origin that just served the pack. `oath get`
shows both in actual. An empty list is not a safety proof.

## Origins

A pack is its hash; it does not know where it is hosted. The machine
says where it looks: `host:local.origins`, a list of `http://` or
`https://` prefixes tried in order as `{origin}/pkg/<name>/<hash>.tar`
until one serves the pinned tree. Apply uses them for a pinned pkg with
no `url`; `oath build` uses them for a build pack the store lacks; `oath add`
and `oath update` read each package's head from every one. The `base`
profile lists the default origin, the R2 bucket at
`https://store.oath.moonlight.pm`, which carries every locked pack
(build packs too) with its plan and source, but of a recipe only the plan (see
Recipes). `INDEX.md` lists names and hashes, and `index.json` is the same list
for `oath ls --origin`. Each package it lists but `linux`, `oath`, `firmware` and
a recipe has a signed head there, `pkg/<name>.json`.

```
oath get host:local                                   # origins
oath ls --origin m4                                   # what they serve
sudo oath add m4
```

By hand, a pin with no `url` comes from the origins when it is applied, and the log
says which one served it:

```
sudo oath new pkg:m4 --from-json '{"present":true,"hash":"sha256-…"}'
sudo oath apply pkg:m4
sudo grep '"fetched"' /oath/log/oath.jsonl | tail -1  # where it came from
```

Some packs are on the origin and in no profile: Node.js (`pkg:node`, which
needs `pkg:gcc`), the GitHub CLI (`pkg:gh`, which needs `pkg:certs` for its CA
roots) and Cloudflare's CLI (`pkg:cf`, which needs `pkg:node`). `oath add` takes
each with the needs the machine lacks: `desk` has `pkg:gcc`, and on a `base` machine
it makes `pkg:gcc` and `pkg:mpc` too:

```
sudo oath add node gh cf
node --version                           # v26.13.0
gh --version                             # gh version 2.103.3 (0000-00-00)
cf --version                             # 1.0.0-beta.26
```

`cf` (also `cloudflare`) carries the npm packages it was pinned with in
its own tree and runs on `pkg:node` (R46), and puts nothing in a global
`node_modules`. `npm install -g` fails as `home`, since the store is root's;
under `sudo` it writes into the pack, which `oath verify pkg:node` then reports
as damaged. `cf` still prints a notice when npm has a newer cf;
that is a new pin in `pkgs/cf`. `gh` runs with its update notice off and
`gh auth login` keeps its settings under `~/.config/gh`. Both send
telemetry unless `DO_NOT_TRACK=1` (or `GH_TELEMETRY=false`,
`CF_SEND_TELEMETRY=false`). `npm` from `pkg:node` takes nothing younger than a
week and runs no package's install scripts (`min-release-age=7`,
`ignore-scripts=true`); a project that needs a script says so in its own
`.npmrc`.

`pkg:python` puts no `pip` in `/bin`, and `python3 -m pip` fails until pip is
installed. The pack carries pip's wheel: `python3 -m venv DIR` installs it in
`DIR`, and `python3 -m ensurepip --user` installs it under `~/.local`, whose
`bin` is not on `PATH`, so it runs as `python3 -m pip`. Under `sudo`,
`python3 -m ensurepip` writes into the pack, which `oath verify pkg:python`
then reports as damaged.

`pkg:sqlite` is the `sqlite3` shell and a static library for programs that embed it
(needs `pkg:zlib`). `pkg:llama-cpp` is `llama-server`, local models over Chat
Completions on the CPU or, through Vulkan, a GPU: it needs AVX2, `pkg:gcc` and
`pkg:vulkan-loader`, and reads a model from a file (`-m`). `pkg:writ` is a coding
agent whose tools run in a box, and `writ serve --model FILE` runs `llama-server`
in a box of its own ([writ](writ.md#local-models)).

Put a LAN mirror first to prefer it (any directory `cargo make publish
--dest` wrote, served over HTTP):

```
sudo oath set host:local origins='["http://10.0.0.5:8080", "https://store.oath.moonlight.pm"]'
sudo oath apply host:local
```

An empty list fetches nothing without a `url`. A failed origin is
skipped; apply fails only when none serves the pin, and says what each
answered.

### Servers: Blocky, restic, route, cell, PostgreSQL, MySQL (R49)

These are built from source here and are in no profile:
`pkg:blocky`, `pkg:cloudflared`, `pkg:restic`, `pkg:rsync`, `pkg:route`, `pkg:cell`,
`pkg:mysql`, `pkg:postgresql` and `pkg:postgis`
(with `pkg:geos`, `pkg:proj`, `pkg:libxml2`, `pkg:json-c`, `pkg:icu`).
`sudo oath add <name>` takes each with what it needs, except PostGIS (below).
`pkg:go` is what builds the Go ones; a machine needs
it only to build. `cloudflared` does not update itself: `cloudflared update` says so,
and a newer one is a new pin in `pkgs/cloudflared`.

`pkg:mysql` is MySQL 9.7, its server and clients in one package (R82). Nothing runs until a
`svc` starts `mysqld` with a `--datadir`. As root, `mysqld` stops unless `--user` names an
account, so its `svc` names a `user`. `LOAD DATA` and `INTO OUTFILE` are off until
`secure_file_priv` names a directory. Not built: Group Replication, NDB, Router, and LDAP,
Kerberos, FIDO, OCI and OpenID logins. A 9.x server has no `mysql_native_password`, so an
account from 8.x moves to `caching_sha2_password`; the client keeps `mysql_native_password`,
to log in to older servers.

Go programs read their CA roots from `pkg:certs` (R50), through Go's own
`crypto/x509`, unless `SSL_CERT_FILE` names others:
`/oath/store/pkg/certs/live/ssl/cert.pem`. It is Mozilla's roots for servers, from
NSS's `certdata.txt`, less any with a distrust-after date and what
`pkgs/certs/distrust.txt` names. curl and OpenSSL read `pkg:curl`'s own copy,
Mozilla's bundle from curl.se, at `/oath/store/pkg/curl/live/ssl/cert.pem`
(OpenSSL's `cert.pem` links to it). It keeps the roots `pkg:certs` leaves out.

PostgreSQL finds PostGIS, a separate pack, through one line in
`postgresql.conf`:

```
extension_control_path = '$system:/oath/store/pkg/postgis/live/share'
```

`pkg:postgresql` and `pkg:postgis` both carry `lib/postgresql/`, which `/lib`
links for one package only, so the second to apply is refused:
`/lib/postgresql exists and is not pkg:postgis`. `sudo oath add postgis` fails either
way: with PostgreSQL here it leaves PostGIS out, and without it applies PostGIS first,
is refused (`/lib/postgresql exists and is not pkg:postgresql`) and leaves PostGIS
without PostgreSQL or PROJ, which `sudo oath undo` takes back. Add PostgreSQL and
PostGIS's other needs first, then make PostGIS's object and apply it:

```
sudo oath add postgresql geos json-c proj
sudo oath new pkg:postgis
sudo oath apply pkg:postgis       # refused: /lib/postgresql exists and is not pkg:postgis
```

The refused apply still leaves PostGIS's `live` link in the store, which is all
the `extension_control_path` line needs, so `CREATE EXTENSION postgis` works.
`pkg:postgis` stays `status=new`, and every `oath apply` with no id stops at it.

The server refuses to run as root, so its `svc` names a `user`. A new major
version changes the data directory's format, so moving across one is
`pg_upgrade`, not a new pin alone.

`pkg:cell`, in `apps/cell`, runs a program in a box of its own, with one uid and no daemon: an
image from a registry, a package this machine has, or a `cell:` object. It is in no profile;
`sudo oath add cell` takes it with the four packages it needs that `base` and `desk` lack
(`crun`, `json-c`, `libseccomp`, `passt`). [Cells](cells.md) is its chapter.

`pkg:route` is Oath's own HTTP router (R59), in `apps/route`, with nothing but Go's
standard library: by host and path, to a local service, a redirect or a fixed answer.
It holds no TLS and belongs on a loopback address, behind a tunnel or other proxy:

```json
{
  "listen": "127.0.0.1:8080",
  "routes": [
    {"host": "a.example", "to": "127.0.0.1:9000"},
    {"host": "a.example", "path": "/admin", "to": "127.0.0.1:9001", "strip": "/admin",
     "request": {"X-Forwarded-Prefix": "/admin", "Cookie": ""},
     "response": {"X-Frame-Options": "DENY", "Set-Cookie": ""}},
    {"host": "a.example", "path": "/gone", "status": 404, "body": "not found\n"},
    {"host": "old.example", "redirect": "https://new.example", "status": 301},
    {"host": "*", "status": 404}
  ]
}
```

The longest path wins, at a segment's edge, and `*` stands in for a host no route
names. `strip` takes a prefix off, then `prefix` puts one on. An empty header value
removes that header. A service gets the path that matched: repeated slashes collapse,
and a path with a `.` or `..` segment gets a 400. `route <file>` runs it,
`route -check <file>` tries a file, SIGHUP reads it again and keeps the old routes if
the new file is bad or moves `listen`, which takes a restart, and SIGTERM drains. A
redirect to a target with no path or query keeps the request's path and query.
Websockets and streams pass through. A forwarded `X-Forwarded-*`
is kept, so it trusts whatever reaches its address.

### AWS CLI (R82)

`pkg:aws` is AWS CLI v2 on `pkg:python`, with its completer and AWS's
`session-manager-plugin`, built from AWS's source and in no profile. It takes roots from
`pkg:certs`. API calls go through `pkg:openssl`; S3's CRT transfers through the AWS-LC inside
`awscrt`. Its libraries stay in `libexec/aws`, out of `/lib` and Python's own
site-packages. `aws help` is AWS's plain text, as Oath has no groff, and `aws update`
refuses: a new version is a new pin.

```
sudo oath add aws
aws login --remote                        # a sign-in URL for any browser
aws ssm start-session --target i-0123…    # runs session-manager-plugin
```

### Minecraft: Prism Launcher (R74)

`pkg:prismlauncher` is Prism Launcher, with `pkg:jre-21` (Java 21) and Qt 6, in
no profile. On a `desk` machine, `sudo oath add prismlauncher` takes it with each need it
lacks (`cmark`, `jre-21`, `libarchive`, `libqrencode`, `libxcursor`, `libxi`, `libxtst`,
`qtbase`, `qtimageformats`, `qtnetworkauth`, `qtsvg`), and `oath update` moves each to its head.
`prismlauncher` starts it in a box: it sees its own folder
(`~/.local/share/PrismLauncher`), the display, sound, the GPU, pads and the
network; not the rest of `/home`, and `sudo` does not work in it, so a mod
cannot reach them. Mods and modpacks install from Modrinth and CurseForge in
its window. Signing in shows a code and a QR Code to use on any device, at
microsoft.com/link; nothing plays until a Microsoft account that owns the game
is added. To bring a Modrinth App profile over, see
[Minecraft](../specs/minecraft.md#bringing-a-modrinth-app-profile-over).

### writ: a coding agent in a box (R64)

`pkg:writ` is Oath's own coding agent, in `apps/writ`, on the origin and in no
profile. It needs `pkg:bubblewrap` and `pkg:curl`, which `desk` has;
`sudo oath add writ` installs it, and on a `base` machine `pkg:bubblewrap` with it.
It has a head on the origin, so `oath update` moves it to its head. [writ](writ.md)
is its chapter.

## Plans

A **plan** (`plan:<name>`) is a hashed `plan.plan` file that builds a
product tree for the `pkg:<name>` or `cell:<name>` that `produces` names:
the line `#!oath-plan`, a JSON head (`name`, `produces`, `build_needs`,
`run_needs`, each need pinned by hash), the line `---`, and a script whose
first two lines are `#!/bin/sh` and `set -eu` and which writes `/out`.
`oath build` runs it in a sandbox with only
the pinned needs mounted and no network. The script is PID 1 of its own
PID namespace, with its own `/proc`, `PATH=/bin` and `HOME=/tmp`; on a
machine (`sudo oath build`) it runs as `home`, uid 1. The sandbox's `/bin` and `/lib` are link
farms of those packs, as on a machine, so a plan pins every pack its
tools load: `apps/hello.plan` pins bash, coreutils, gcc and glibc, and
gmp, mpfr and mpc, which they run with.

```
oath fmt -w apps/hello.plan
sudo oath store pkg:hello-src apps/hello-src     # the source no origin carries
sudo oath build --file apps/hello.plan
sudo oath set plan:hello --from-file apps/hello.plan
sudo oath build plan:hello
sudo oath set pkg:hello present=true
sudo oath apply pkg:hello
```

`plan:<name>` has one field, `hash`, the pin of the canonical file:
`oath set plan:<name> --from-file` lints the file, stores it under
`/oath/store/plan/<name>/<hash>/`, and makes or updates the object, pinned
at once with no `oath apply`. Empty is unpinned, and `oath build plan:<name>`
refuses it. Actual adds the last build's `product` and `inputs` hashes;
neither is a proof.

A `build_needs` pack the store lacks is fetched first, from its `pkg`
object's `url` or else from `host:local.origins`; it needs no `pkg` object.
A second `oath build plan:<name>` of an unchanged input set is a no-op; `--file`
always builds. A build with no `pkg:<name>` object makes one, absent, at the
product's hash, with the plan's `run_needs` as its `needs`
(`oath build plan:<name> --no-pkg` makes none, for a product that is only another
plan's build input).
The script's output goes to `/oath/log/plan/<name>.log`, not the
terminal: `oath build` prints the path (and a failed build the last
lines), and `oath logs plan:hello` reads it. The previous build's log
is `hello.log.1`.

The build cache is keyed on the plan hash and the input set, so an
edited script always rebuilds.

`apps/hello.plan` pins `pkg:hello-src`, which no origin carries, so a machine
stores it first (`sudo oath store pkg:hello-src apps/hello-src`); its other needs
come from the origins, so it builds on a machine with no compiler in `/bin`. Its
`pkg:gcc` and `pkg:glibc` pins are older than a release's, and a build that
fetches a need moves that package's pin and `live` to the need's hash: where a
present package needs one at the release's pin, as on a desk, the next apply
refuses until `oath set` puts it back. The plans in `pkgs/` are the larger
examples.

## Cells

A **cell** (`cell:<name>`) is one project's own software: a tree under
`/oath/store/cell/<name>/<hash>/`, never linked into `/bin` or `/lib`, from no
origin and never published, which only `cell run` from `pkg:cell` starts.
[Cells](cells.md) is its chapter: the fields, `oath store`, a plan that makes
one, and what `oath reap` keeps.

## Recipes

A package whose licence does not allow redistribution is a **recipe**
(R23), and so is a coding agent (R31): the image holds its plan, not
its pack, and the machine builds the pack itself from the vendor's own
file. `oath` (INDEX.md) lists
the recipes under **Recipes** until their package is present. Claude
Code is one:

```
sudo oath build plan:claude-code         # fetches ~240 MB from downloads.claude.ai
sudo oath set pkg:claude-code present=true
sudo oath apply pkg:claude-code
claude --version                         # 2.1.311 (Claude Code)
```

`claude doctor` then warns that `~/.local/bin/claude` is missing: that
is where the vendor's own installer puts it, and Oath does not use it.

`pkg:claude-code-src` is the vendor's file: `url` is the file itself
and `file` is where it goes in the tree (`src/claude`); `hash` pins
that tree, so a changed download is refused. The plan builds with
packs every machine has (bash, coreutils, grep, glibc) and must make
the very pack the build host made: `pkg:claude-code`'s `hash` is that
pin, and apply refuses another. `claude` runs with `DISABLE_UPDATES=1`;
a newer Claude Code is a new pin in `pkgs/claude-code`, and `oath update`
builds it on a machine where `pkg:claude-code` is present, as for every
recipe the owner built (below). Its settings, login and history are under `/home/.claude` and `/home/.claude.json`.

Codex (OpenAI) and Grok Build (xAI) are recipes the same way (R31),
though their licences would allow a pack:

```
sudo oath build plan:codex               # fetches 144 MB from github.com
sudo oath set pkg:codex present=true
sudo oath apply pkg:codex
codex --version                          # codex-cli 0.166.0

sudo oath build plan:grok-build          # fetches 57 MB from x.ai
sudo oath set pkg:grok-build present=true
sudo oath apply pkg:grok-build
grok --version                           # grok 1.0.70 (f50904eaee6c)
```

`codex` is OpenAI's package as it ships, with its own ripgrep and its
own bubblewrap, which sandboxes the commands it runs; it starts with its
update check off. `codex login` signs in; its settings and sessions are
under `/home/.codex`. OpenAI signs the package with Sigstore, and the
build host checks that signature. `grok` sets `auto_update = false` in
`~/.grok/config.toml` before Grok starts, since that is the only switch
its updater has. `grok login` signs in; its settings are under
`/home/.grok`. xAI publishes no checksum or signature, so the pin is
what the build host downloaded. Grok looks for `bwrap` only under
`/usr`, so on Oath it runs commands without its sandbox.

A recipe has no head: `oath add` and `oath new pkg:<name>` with no fields refuse it,
and `oath ls --origin` ends its line with ``(a recipe: `oath build` makes it here)``.
`oath update` makes a recipe new to the profile, absent, and files its plan.
One it lists as `in the output's seed, not here, left to the owner (oath new)`
takes `oath new pkg:<name>-src --from-json` and `oath new pkg:<name> --from-json`
with the output's documents, and `oath set plan:<name> --from-file` with its plan
from the output.

Steam is another, in the `desk` profile (R29): `pkg:steam` is Valve's
launcher, from repo.steampowered.com, checked against Valve's signed
`.dsc`. Its needs (`bubblewrap`, `libxcrypt`, and the 32-bit `glibc-i386`,
`libxcrypt-i386`, `mesa-i386` and `libglvnd-i386`) are present on the desk. A
machine whose catalog lacks the objects makes the ones an update lists as left
to its owner (`pkg:steam pkg:steam-src plan:steam`) as above. Then:

```
sudo oath build plan:steam               # fetches 20 MB from repo.steampowered.com
sudo oath set pkg:steam present=true
sudo oath apply pkg:steam
steam                                    # on the desk; DISPLAY defaults to :0
```

`steam` starts Valve's launcher in a sandbox that gives Steam the
root it assumes (`/usr`, `/lib64`, `/lib32`, `/sbin/ldconfig`, an
ld.so cache, `/etc/ssl`) and leaves the machine's root as it is. On a
first run it opts into Valve's 64-bit client (the beta channel), then
downloads the client (about 1 GB) into `~/.local/share/Steam`, where
Steam updates itself; Oath pins only the launcher. Its logs are
`~/.local/share/Steam/logs/console-linux.txt`; the sandbox's tree is
`$XDG_RUNTIME_DIR/oath-steam/`.

`oath update` builds a recipe the owner built again, from the output's
plan, before it changes anything, and moves it in the same apply: Steam
when its plan or its needs move (the 32-bit Mesa), NVIDIA's driver when
`pkg:linux` moves.

NVIDIA's driver is another, in `desk` (R34), for a machine with an
NVIDIA card. Its build compiles the kernel modules against
`pkg:linux-build`, so an update that moves `pkg:linux` builds it
again, to the new kernel:

```
sudo oath build plan:nvidia              # fetches NVIDIA's 420 MB .run from download.nvidia.com
sudo oath set pkg:nvidia present=true
sudo oath apply pkg:nvidia
nvidia-smi                               # once its modules load (the pass after services, or the next boot)
```

libglvnd sends GL and EGL to it, the Vulkan loader and libgbm (32-bit
too) find it, and its 32-bit libraries serve Steam. Its DLSS DLLs are
in `/lib/nvidia/wine`, beside `libGLX_nvidia`, where Proton finds them;
a game also needs `PROTON_ENABLE_NVAPI=1 %command%` unless Proton turns
NVAPI on for it. The closed driver runs on the GSP firmware in its
package, which PID 1 puts where the kernel looks. A card's own settings (GSP off,
power caps) belong in the machine's overlay ([Kernel module
options](using.md#kernel-module-options)).

## Security audit

`cargo make audit` (R48) matches every package `pkgs/` builds, and the
Rust crates in `pkg:oath`, against cve.org's CVE List V5 (a local mirror
in `~/.local/share/oath/cvelist`, fetched each run), OSV, NVD, CISA's
Known Exploited Vulnerabilities and EPSS, and writes
`build/audit/latest.json` and `build/audit/<day>.json`. `--offline` keeps the
mirror as it is and skips NVD; `--name <pkg>` scans one and writes no report. A
hit on one of OSV's `MAL-` records fails the scan. `OATH_CVELIST` names another
mirror directory. The scan waits 6.5 s after each NVD query, or 0.7 s with
`NVD_API_KEY`, from the environment or `~/.config/oath/nvd.env`
(`NVD_API_KEY=…`, never committed). What it matches each package by is
`pkgs/audit.toml`: its NVD CPE, the names CVE records use, its upstream
repo and release tag (OSV's commit query), an OSV ecosystem name, or why
there is nothing to match; no plan reads that file, so an edit there
moves no hash. A CVE listed under `[<name>.not_affected]`, or one a
`pkgs/<name>/patches/*CVE-….patch` fixes, is listed apart.

A CVE record can come days after the release that fixes it, so the scan
also reads each pin against upstream's newest stable release: from its
`repo` and `tag` by `git ls-remote`, else from the directory its source
comes from. Each package gets `newer`, or `upstream_unread` and why;
`cargo make audit --upstream` is that check alone (under a minute, and it
writes nothing). `series` holds a package to a release line, `dev_odd` and
`dev_micro` mark development releases, and `unchecked` says why a package
cannot be read. A newer release is a prompt to read its notes, not a
finding: only one that names a security fix moves the pin.

On buccina, `svc:audit` runs it daily at midnight through `check/audit/run`:
a headless Claude Code run of the `oath-audit` skill, in a box
(`check/audit/box`, R77), triages the hits, upgrades to upstream's fixed release
(or applies upstream's patch, or a backport marked `oath-backport-CVE-…`), commits
and checks. Outside the box the runner then lands those commits, pushes and
publishes, mails the report and moves buccina to the release with a reboot;
a run that ends at 07:00 or later leaves that move to the next run that ends before
07:00. Each run also puts the findings on the origin (`cargo make audit --publish`).
The box holds no key but its own Claude token, `~/.config/oath/claude-token` (mode 600), which
lasts a year. Without it the run mails that it did not run; to make one, run
`claude setup-token` and save what it prints there.
Every run, fix or no fix, it also runs `cargo make pkgs --stale`, which lists
each package built against something the lock has since replaced, and rebuilds
those (R75); a command may last 6 hours.

On a machine, `oath audit [pkg:<name>]` reads the report
`cargo make audit --publish` put on the origin (`audit/latest.json`), from the
first origin whose copy a key in `host:local.publishers` signed, and lists what
it found for the packages present here: a finding per line
(`pkg:<name> <version>: <id> <severity> <status>`, with `known exploited` and
`fixed in <version>` when the report says so) and each newer upstream release
(`pkg:<name> <version>: upstream has <version>`). A package at another version
than the audit read is `not assessed`,
which is not clean; an `-i386` package is read without that check. A source that
failed says so before the findings, and the last lines say how many packages were
read and which are not in the report. `--json` is the report, with `origin` and
`signer`; an id narrows it to that package. It exits 0 whatever it finds.

## Host store cache and publishing

The build host keeps the same layout under `~/.local/share/oath/store` (or
`$OATH_STORE`), outside the checkout:

```
cargo make store --name hello --from apps/hello --tar
cargo make advisories            # OSV lookup from components.json (OATH_OSV for another endpoint)
cargo make advisories --name <n> # or only that package
cargo make audit --publish       # the audit's latest.json, signed, on the origin (R61)
cargo make publish               # upload to the default origin what it lacks
cargo make publish --dest <dir>  # or write the same tree into a directory
```

To the origin, `publish` goes only from a clean `master` that is
`origin/master` (it fetches first), and only with an output built from
a clean tree at a commit master has (R47): commit, push and `cargo make
build` again otherwise; nothing overrides it. An OSV `MAL-` record against a
Go, npm, PyPI or Cargo component of a locked pack stops it too, and nothing
overrides that either. `--dest` is not checked.

`publish` carries every pack `pkgs/lock.json` records, build packs
included (`gcc`, `rust`, `python`, …), and every package a seed profile
names, except one it pins from elsewhere, each at the hash this tree records for it: the product in
`pkgs/lock.json`, or `pkg:oath`'s last `cargo make build`. Each needs a
licence on record: `lock.json`'s, or for `pkg:oath` the workspace
licence AND every crate's in its `components.json`. Each pack goes out
with its source: the plan that built it (`plan/<name>/<plan
hash>.plan`) and the source pack the plan pins
(`pkg/<name>-src/<hash>.tar`). It refuses, before writing anything, a
package that is not built, has no licence, has no plan matching the
record, needs a package that is not published, or whose plan builds
with a pack not published at the hash it pins (the lock's, or one the
origin already has from an earlier publish, as CEF's earlier build of OpenSSL): an
origin alone rebuilds every plan on it. `INDEX.md` lists name, hash, licence, plan and
source, and `index.json`, which `oath ls --origin` reads, the same rows with version,
category, description and size; `INDEX.md` goes up last. A recipe goes out as its plan
only, listed with the product hash it builds. Test packs and other hashes in the cache
are never published; one line names the packages left out.

`publish` also signs a head ([Publishing a package on its
own](#publishing-a-package-on-its-own-r72)) for every package it carries but
`linux`, `oath` and `firmware`, which move with a release: `pkg/<name>.json`,
with the lock's pin, its plan's `run_needs`, the version, description, home and
category, and the commit's time, and a `.sig` from the build host's signing key
(below). A head already there that names the same pack and needs, or that comes
from a later commit, stays as it is. A recipe has no head. Each head goes up after
its pack and before `index.json`. A machine whose `host:local.publishers` trusts
that key for the name takes these heads at `oath update` and `oath add`, whatever
profile it follows.

To the default origin, `publish` lists the bucket, uploads only the
hashed files it lacks (`pkg/<name>/` and `plan/` never change) plus the
heads it signed (`pkg/<name>.json` and its `.sig`), `adv/`, `index.json`
and `INDEX.md`, and reads `INDEX.md` back from
`https://store.oath.moonlight.pm`. The write key is on a publisher's machine
alone: `OATH_ORIGIN_S3` (`https://<account>.r2.cloudflarestorage.com/oath-store`),
`OATH_ORIGIN_KEY_ID` and `OATH_ORIGIN_SECRET`, from the environment or
`~/.config/oath/origin.env` (mode 600, never committed). They are an R2
S3 key: a Cloudflare account token allowed to read and write objects
in `oath-store` only; its id is the key id and the SHA-256 of its value
the secret.

Then `publish` puts out the newest output `cargo make check` passed
(`--output <id>` for another), which machines update to (R38,
[Install](install.md)): `out/<id>/update.tar`, `out/<id>.json` and
`out/<profile>.json`, each `.json` with a `.sig` from the build host's
signing key, `~/.config/oath/sign_ed25519` (`OATH_SIGN_KEY`; make one
with `ssh-keygen -t ed25519 -N '' -C oath@<host> -f
~/.config/oath/sign_ed25519`). Machines take it only if that key's
public half is in their `host:local.publishers`. It refuses an output
whose packs the origin lacks (a pack pinned from elsewhere is its
owner's to publish), and an output already published stays as it was,
so publishing an older one again never moves a profile back.

Optional backup hooks in a pack (not catalog fields):
`libexec/oath-backup-quiesce` and `libexec/oath-backup-thaw`. See
[Services](services.md).
