# Cells

**Status:** an installed package runs in a box (R76) and `cell:<name>` is a kind (R78),
both released 2026-11-09. Images run as they did (R59). A project's
path from source to a `cell:` on a host is not built.

A cell is one program in a box of its own: namespaces through `crun`, a network
through `pasta`, one uid (R53), no daemon. The code is `apps/cell`, on Go's standard
library alone.

## Built

### A package, with no image (R76)

`cell run [flags] pkg:<name> [args...]` runs a package this machine has installed. It
takes the flags an image's run takes, and `--with pkg:<other>` for one more package.

- **What it sees.** The package, what it needs, `--with`, and a floor of `glibc`,
  `bash`, `coreutils` and `curl`. Each is at the hash this machine links, from `oath
  --json get`. It does not see the machine's `/etc`, catalog, `/home`, `/work` or other
  packages.
- **How.** Each pack is bound read-only, `nosuid` and `nodev`, at
  `/oath/store/pkg/<name>/<hash>`, with a `live` link beside it, because programs name
  that path. `/bin` and `/lib` are the cell's own links, made by a machine's rules
  (`bin/`, and `lib/` less `oath`, `modules`, `firmware` and `pkgconfig`). The first
  pack to name an entry has it, and the package comes first.
- **Its program.** `bin/<name>`, or what the command line names.
- **What it writes.** Its own root, which goes when it stops. What should last goes to
  a `-v` volume.
- **`--user uid[:gid]`.** The cell's `/etc/passwd` names the number, for a program
  that looks itself up, as PostgreSQL does.
- **What it costs.** `pkg:python` as `work` on buccina, 2026-11-09: 13 packs, 264
  links, 28 KB of root, and a one-shot run in 0.1 s.

A `svc` runs it like any program, as `work`, with `/bin/cell` as its `exec`.

### A project's own software (R78)

`cell:<name>` is a kind beside `pkg`, for software that belongs to one project. `cell
run cell:<name>` is the same box as a package's, with the tree bound read-only at
`/oath/store/cell/<name>/<hash>`.

- **It stays home.** It is not linked into `/bin` or `/lib`, comes from no origin, and
  `oath publish` refuses it. Those are `pkg:` paths, so nothing has to remember to skip
  one. Its name cannot clash with a package's.
- **Its needs are named, not pinned.** `needs` are `pkg:` ids with no hash: a cell starts
  with each at the hash this machine links, so an update that moves a package leaves
  nothing to re-point. Apply refuses a need the machine lacks and refuses to remove one a
  present cell needs, and an update holds what a present cell needs when a release drops
  it.
- **How it gets there.** `sudo oath store cell:<name> <dir|tar>`, `oath new` with `hash`
  and `needs`, then `oath apply`. A plan can make one: `"produces": "cell:<name>"`, built
  from `cell:<name>-src`. A package's plan cannot name a `cell:`, since its source is
  published with it.
- **Damage.** A tree gone from the store, or one `oath verify` found no longer hashes
  to its pin, drifts in `tree`. A cell comes from no origin, so apply refuses it before
  it takes a generation, undo refuses to put one back before it changes anything, and
  `sudo oath store cell:<name> <dir|tar>` of the same files is the repair: it records
  the check in the cell's actual as verify would, so the drift, the `error` status and
  INDEX's store damage clear with no apply. A cell whose tree was gone at boot is not
  linked, so `cell run` refuses it, and it drifts in `tree` until an apply links it,
  whatever verify or the store says of the tree once it is back.
- **What goes.** Reap keeps the pin and lets a cell's other versions go, as for a package.

### An image (R59)

`cell run <image>` pulls a registry's image if it is not on disk, and runs a copy of
it. This stays for what other people publish, and for a project until it is ported.

### Both at once

Images, packages and `cell:` objects share one binary, account, store and set of
commands: a runner changes only the name it passes. An nginx image and `pkg:python` ran
side by side as `work` on buccina on 2026-11-09, each serving a volume on a published
port, with `exec`, `ls` and `stop`; a `cell:` did the same as `home`, against a scratch
copy of its catalog.

## Known defects

- `pkg:openssl`'s default CA path is a link to `pkg:curl`'s bundle, and its `run` does
  not name curl. The floor carries curl for that reason.
- A program that is first in its box and has no handler for SIGTERM ignores it, so
  `cell stop` waits out `--stop-after` (10 s) for SIGKILL. `python3 -m http.server` did.

## Target

A project outside Oath's tree becomes a `cell:`: `pkg.toml` and `build.sh`, built by
`oath build` with no network, its locked dependencies fetched first and checked against
the lock's own hashes. No image is made.

`home` builds, on any machine, and installs on the machine that runs it, over SSH with
`oath store`, `oath new` or `oath set`, and `oath apply`. A package the machine lacks is
installed first. `work` only runs.

Nothing on Oath builds an image (R80): a project that must ship before its port ports
first.
