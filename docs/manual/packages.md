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
| `pkg:openssh` | the SSH server `sshd` (R36), and `ssh`, `scp`, `sftp`, `ssh-keygen`, `sftp-server` |
| `pkg:zsh` | `zsh`, `home`'s login shell |

What the running `oath`'s base profile fixes is what counts: a package a
machine was seeded with as fixed and the base no longer fixes
(`pkg:dropbear`, before R36) can be removed.

Present and removable: `sed`, `grep`, `gawk`, `findutils`, `diffutils`,
`tar`, `gzip`, `xz`, `bzip2`, `procps` (`ps`, `pidof`, `free`, `top`,
`kill`), `less`, `vim` (also `vi`), `iputils` (`ping`, `tracepath`,
`arping`), `chrony` (`svc:time`), `dosfstools` (`mkfs.fat`), `curl`
(Mbed TLS, CA bundle at `/oath/store/pkg/curl/live/ssl/cert.pem`),
`bind` (`dig`, `host`, `nslookup`; no `named`), `openssl` (`openssl`;
its default `ssl/cert.pem` links to curl's bundle), and the libraries they load:
`ncurses`, `gmp`, `mpfr`, `zlib`, `zstd`, `lzo`, `mbedtls`, `libuv`,
`liburcu`, `libcap`, `libxcrypt`. A pack's `needs` lists the libraries it loads, so
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
stays, so a re-install needs no network. A new `hash` relinks, and a
program the new tree no longer has leaves `/bin`. Apply refuses to clobber a
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
  With no `url`, a pinned pack the store lacks comes from the machine's
  origins (below).
- `file`: with `url`, the URL is one file, a vendor's release, and the
  tree is that file alone at this path, not executable (a recipe's
  source; see Recipes).
- `needs` lists other packs this one needs at runtime, each pinned by
  hash. Apply refuses a broken graph: a missing need, a need at the
  wrong hash, a cycle, or removing a need while a present pack still
  lists it. Apply does not change the other objects for you.
- `description` and `home` are prose and a project URL.

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

A new name is `oath new pkg:hello hash=…` instead of `set`; a source
pack a plan builds with is `oath new pkg:hello-src hash=… present=false`
and goes in the plan's `build_needs`. Every file is stored 0755 or 0644
(the hash records only whether it is executable), and a pipe, socket or
device in the tree is refused, as a fetch refuses it.

A service follows its package: when an apply moves the pin of the
package a service's program comes from (`exec[0]`, through `/bin`),
PID 1 restarts that service, and no other (an update leaves the
desk's running: [Services](services.md)). So a change to a program is
build, `oath store`, `oath set … hash=`, `oath apply`.

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
no `url`; `oath build` uses them for a build pack the store lacks. The
`base` profile lists the default origin, the R2 bucket at
`https://store.oath.moonlight.pm`, which carries every locked pack
(build packs too) with its plan and source; its `INDEX.md` lists names
and hashes.

```
oath get host:local                                   # origins
curl -fsS https://store.oath.moonlight.pm/INDEX.md    # what it serves
sudo oath new pkg:m4 --from-json '{"present":true,"hash":"sha256-…"}'
sudo oath apply pkg:m4
sudo grep '"fetched"' /oath/log/oath.jsonl | tail -1  # where it came from
```

Some packs are on the origin and in no profile, and a machine adds them
this way: Node.js (`pkg:node`, which needs `pkg:gcc`), the GitHub CLI
(`pkg:gh`, which needs `pkg:curl` for its CA bundle) and Cloudflare's
CLI (`pkg:cf`, which needs `pkg:node`). Each need is pinned at the hash
the pack's plan names (`run_needs` in `{origin}/plan/<name>/<plan>.plan`),
which must be the one the machine runs:

```
sudo oath new pkg:node --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:gcc","hash":"sha256-…"}]}'
sudo oath apply pkg:node
sudo oath new pkg:gh --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:curl","hash":"sha256-…"}]}'
sudo oath apply pkg:gh
sudo oath new pkg:cf --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:node","hash":"sha256-…"}]}'
sudo oath apply pkg:cf
node --version                           # v26.13.0
gh --version                             # gh version 2.103.3 (2026-10-26)
cf --version                             # 1.0.0-beta.26
```

`cf` (also `cloudflare`) carries the npm packages it was pinned with in
its own tree and runs on `pkg:node` (R46); nothing goes in a global
`node_modules`, and `npm install -g` has nowhere to write, since the
store is read-only. `cf` still prints a notice when npm has a newer cf;
that is a new pin in `pkgs/cf`. `gh` runs with its update notice off and
`gh auth login` keeps its settings under `~/.config/gh`. Both send
telemetry unless `DO_NOT_TRACK=1` (or `GH_TELEMETRY=false`,
`CF_SEND_TELEMETRY=false`).

Put a LAN mirror first to prefer it (any directory `cargo make publish
--dest` wrote, served over HTTP):

```
sudo oath set host:local origins='["http://10.0.0.5:8080", "https://store.oath.moonlight.pm"]'
sudo oath apply host:local
```

An empty list fetches nothing without a `url`. A failed origin is
skipped; apply fails only when none serves the pin, and says what each
answered.

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
`apps/hello.plan` builds on the build host (in `~/.local/share/oath`), not on
the box. The plans in `pkgs/` are the larger examples.

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

Codex (OpenAI) and Grok Build (xAI) are recipes the same way (R31),
though their licences would allow a pack:

```
sudo oath build plan:codex               # fetches 117 MB from github.com
sudo oath set pkg:codex present=true
sudo oath apply pkg:codex
codex --version                          # codex-cli 0.164.0

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
`/usr`, so on Oath it runs commands without its sandbox. A machine
installed before these recipes gets their objects the way the update
names them: `oath new pkg:<name>-src --from-json` and `oath new
pkg:<name> --from-json` with the output's documents, and `oath set
plan:<name> --from-file pkgs/<name>/<name>.plan`.

Steam is another, in the `desk` profile (R29): `pkg:steam` is Valve's
launcher, from repo.steampowered.com, checked against Valve's signed
`.dsc`. Its needs (bubblewrap, glibc for i386, libxcrypt) are present
on the desk. A machine whose catalog lacks the objects (an update
reports `pkg:steam pkg:steam-src plan:steam`) gets them as `oath new
pkg:steam-src --from-json …` with the output's document and `oath set
plan:steam --from-file pkgs/steam/steam.plan`. Then:

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

NVIDIA's driver is a third, in `desk` (R34), for a machine with an
NVIDIA card. Its build compiles the kernel modules against
`pkg:linux-build`, so an update that moves `pkg:linux` builds it
again, to the new kernel:

```
sudo oath build plan:nvidia              # fetches NVIDIA's 420 MB .run from download.nvidia.com
sudo oath set pkg:nvidia present=true
sudo oath apply pkg:nvidia
nvidia-smi                               # once its modules load (the pass after services, or the next boot)
```

libglvnd sends GL and EGL to it, the Vulkan loader and libgbm find it,
and its 32-bit libraries serve Steam. A card's own settings (GSP off,
power caps) belong in the machine's overlay ([Kernel module
options](using.md#kernel-module-options)).

## Host store cache and publishing

The build host keeps the same layout under `~/.local/share/oath/store` (or
`$OATH_STORE`), outside the checkout:

```
cargo make store --name hello --from apps/hello --tar
cargo make advisories            # OSV lookup from components.json
cargo make publish               # upload to the default origin what it lacks
cargo make publish --dest <dir>  # or write the same tree into a directory
```

`publish` carries every pack `pkgs/lock.json` records, build packs
included (`gcc`, `rust`, `python`, …), and every package a seed profile
names, each at the hash this tree records for it: the product in
`pkgs/lock.json`, or `pkg:oath`'s last `cargo make build`. Each needs a
licence on record: `lock.json`'s, or for `pkg:oath` the workspace
licence AND every crate's in its `components.json`. Each pack goes out
with its source: the plan that built it (`plan/<name>/<plan
hash>.plan`) and the source pack the plan pins
(`pkg/<name>-src/<hash>.tar`). It refuses, before writing anything, a
package that is not built, has no licence, has no plan matching the
record, needs a package that is not published, or whose plan builds
with a pack not published at the hash it pins (the lock's, or one the
origin already has from an earlier publish, as CEF's older Mesa): an
origin alone rebuilds every plan on it. `INDEX.md` lists name, hash, licence, plan and source,
and goes up last. A recipe goes out as its plan only, listed with the
product hash it builds. Test packs and other hashes in the cache are
never published; the last line names what was left out.

To the default origin, `publish` lists the bucket, uploads only the
hashed files it lacks (`pkg/` and `plan/` never change) plus `adv/` and
`INDEX.md`, and reads `INDEX.md` back from
`https://store.oath.moonlight.pm`. The write key is the build host's
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
