# Cells

**Status:** an installed package runs in a box (built 2026-11-09, R76, in no release
yet). Images run as they did (R59). A project's path from source to a pack on a host
is not built.

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

### An image (R59)

`cell run <image>` pulls a registry's image if it is not on disk, and runs a copy of
it. This stays for what other people publish, and for a project until it is ported.

### Both at once

Images and packages share one binary, account, store and set of commands: a runner
changes only the name it passes. An nginx image and `pkg:python` ran side by side as
`work` on buccina on 2026-11-09, each serving a volume on a published port, with
`exec`, `ls` and `stop`.

## Known defects

- `pkg:openssl`'s default CA path is a link to `pkg:curl`'s bundle, and its `run` does
  not name curl. The floor carries curl for that reason.
- A program that is first in its box and has no handler for SIGTERM ignores it, so
  `cell stop` waits out `--stop-after` (10 s) for SIGKILL. `python3 -m http.server` did.

## Target

A project outside Oath's tree becomes a package: `pkg.toml` and `build.sh`, built by
`oath build` with no network, its locked dependencies fetched first and checked against
the lock's own hashes. The result is a pack, and a cell runs it. No image is made.

`home` builds, on any machine, and installs on the machine that runs it: over SSH with
`oath store`, `oath set` and `oath apply`, or from an origin. `apply` refuses a need at
another hash than the machine's, so the pin it sets names the target's hashes, and a
need the machine lacks is installed first. `work` only runs.

Whether a project may ship a new image before its port is Open in
[../decisions.md](../decisions.md).
