# Build and QEMU

Everything is built on the build host with `cargo make` (crate
`oath-make`), from a plain shell.

Built here: the kernel; the toolchain (GCC, binutils, glibc); every
package in `pkgs/`, each from source or from the vendor's own release
pinned by sha256 (`pkg:rust`, `pkg:firmware` and the `font-*` packs
among them); PID 1 and its helpers (`oath-init`, `serial-login`,
`screen-login`, `login`, `sudo`), `oath`, and the UEFI loader `oath-efi`
(`pkg:oath`, built by `pkg:rust`). A profile can also ship a pack built
elsewhere, pinned by hash (R28).

Nix is optional. With `nix-build` on `PATH`, `image/host.nix` gives QEMU
and its UEFI firmware for running images; with `nix-shell`,
`pkgs/bootstrap.nix` gives the compilers that build stage 0 of the
toolchain. Without them these come from `PATH`, with OVMF from QEMU's
`share/qemu`. The host's `cargo` builds `oath-make`. None of it ships.

An Oath machine can be the build host too (R32): its own gcc builds
stage 0, its `pkg:rust` builds `oath-make` (a `desk` machine has both;
`base` has neither), and `pkg:qemu` runs images, with the edk2 firmware
from QEMU's release as OVMF. `home` has `/dev/kvm` there (R33); without
it QEMU runs without KVM. Without `qemu-system-x86_64` or `qemu-img` on
`PATH`, `build`, the `check` cases in QEMU and the QEMU rehearsals stop
and say so. No profile has `pkg:qemu` or the `pkg:libslirp` it loads.
`oath add` takes `pkg:qemu` at its head, with each package it needs that
the machine lacks, and applies them ([Packages](packages.md)): on a
`desk` machine that is `pkg:libslirp`; `base` also takes `pkg:gcc`,
`pkg:glib`, `pkg:pixman` and what they need.

```sh
sudo oath add qemu
```

## The toolchain and packages

```sh
cargo make bootstrap        # once: stage 0 + stage 1 toolchain (an hour or more)
cargo make bootstrap -j 8   # bootstrap with eight jobs (default: half the cores)
cargo make pkgs             # every package: the base until it settles, then the rest
cargo make pkgs -j 4        # the same, four packages at a time after the base
cargo make pkgs --list      # versions, licences, what pkgs/lock.json pins
cargo make pkgs curl        # one package (and what it builds with)
cargo make pkgs --stale     # each package built against something the lock has replaced; builds nothing
cargo make pkgs --fetch curl     # fetch and check a pin's sources; builds nothing (no name: all)
cargo make pkgs --review restic  # write what to read for each new or moved Go, npm, Python or Rust package with no verdict (build/review/)
```

`bootstrap` builds a cross binutils, GCC and glibc with the host's gcc
(stage 0, never shipped), then with stage 0 the base packs (stage 1):
`pkg:gcc` (GCC and binutils), `pkg:glibc`, and bash, coreutils, sed,
grep, gawk, findutils, diffutils, patch, tar, gzip, xz, bzip2 and make.
`pkgs` builds each `pkgs/<name>/` as an `oath build` plan in the
sandbox (no network, only the declared packs, glibc's loader and every
pack's libraries under `/lib`), writes the exact plan to
`pkgs/<name>/<name>.plan` and the product to `pkgs/lock.json`. Every
plan but a recipe's builds with the base packs (a recipe's builds with
what its `pkg.toml` names); the base packs rebuild themselves this way
too. Build packs (python, meson, ninja, m4, flex, bison, bc, perl,
elfutils) are not in the `base` image; `desk` ships all of them but
flex and bc. `pkg:rust` is the vendor's Rust release (pinned by sha256,
its binaries pointed at `/lib`'s loader); it builds `pkg:oath`, static
against glibc. The image takes every userspace package from the lock
but `pkg:oath` and what its profile pins (below). How packages are
written: `pkgs/README.md`.

`--fetch` is the gate for a new pin. It checks each source's sha256 and
signature, sets the tarball against the git tag `pkgs/audit.toml` names
and writes the result to `pkgs/tags/<name>.txt`, and lists each Go, npm,
Python or Rust package the pin brings in `pkgs/intake/<name>.txt`. It
stops a release, or such a package, under a week old unless
`age_because` in `pkgs/audit.toml` names the advisory, and a new package
name, or a new publisher of an npm or Rust package, until
`pkgs/intake/approved.txt`, a person's file, lists the package.
`--review <names>` writes under `build/review/` the diff and
what looks like new reach for each package of those pins that is new or
moved and has no verdict in `pkgs/reviews.txt`; `cargo test` fails on
one with no verdict there. An OSV `MAL-` record stops
`cargo make publish` to the origin, with no override.

## The kernel

There is one kernel, `pkg:linux`: vanilla kernel.org Linux, pinned by
sha256 in `pkgs/linux/pkg.toml`, plus `pkgs/linux/patches/` and the
config fragment `pkgs/linux/linux.fragment` over `defconfig`. It is a
plan product like every other package, built by `pkg:gcc` with the
`flex`, `bison`, `m4`, `bc`, `perl`, `pkgconf`, `zlib` and `elfutils`
(libelf, for objtool) packs.

```sh
cargo make pkgs linux
```

The product holds `boot/vmlinuz`, `boot/config`, `boot/Module.symvers`
and `lib/modules/<release>/` with the depmod index (`modules.dep`,
`modules.alias`, `modules.softdep`) written from each module's
`.modinfo`. `cargo make` takes it from `pkgs/lock.json` (or
`OATH_KERNEL` / `OATH_MODULES`). Firmware for the initrd comes from
`pkg:firmware`, not the kernel build. Packing refuses a bzImage whose
release does not match the module directory or the `vermagic` of
`btrfs.ko`. Bump the version and the sha256 together, in `pkgs/linux`
and `pkgs/linux-build` (the kernel's build tree for modules built
outside it). `pkgs/glibc` pins its own kernel tarball for the UAPI
headers.

## The image

Needs: the packs in `pkgs/lock.json` (`pkg:rust` among them), QEMU
(above), and `mkfs.btrfs`, `unshare` and `curl` on the host; no root.
`shell.nix` has `mkfs.btrfs`; every profile has all three. The build
host's Rust compiles only `oath-make`.

```sh
cargo make build                  # profile base → build/out/<id>/
cargo make build --profile desk   # another seed profile (crates/oath-core/seed/)
```

`build` builds `pkg:oath` from this tree: it packs the workspace and
every crates.io crate in `Cargo.lock` (fetched once into `build/fetch`,
checked against the lock's sha256) and runs `cargo build` with
`pkg:rust` in the sandbox, offline. A tree that has not changed is not
rebuilt; `cargo make pkgs oath` does this step alone. Then `build`
writes the boot initrd and the
installer initrd, stages the root tree, seeds `/oath` from the profile,
packs the profile's packages into the hashed store (also cached under
`~/.local/share/oath/store`), and writes a **build output**:

```
build/out/<id>/
  manifest.json          what the image ships; its hash is <id>
  oath.qcow2             btrfs, subvolume @
  bzImage  initrd.gz  BOOTX64.EFI (oath-efi)
  initrd-install.gz  initramfs-install/   the installer ramdisk
  stage/                 the root tree install copies
  check-pass             written by a clean `cargo make check`
build/out/latest -> <id>
```

A build packs into `build/out/.<id>.part/` and renames it when it is
complete; after that only `check` adds `check-pass`. The id is the
first 12 hex digits of the manifest's SHA-256 (without the commit), so
a tree that has not changed reuses its output in seconds. The newest
five outputs are kept, and the newest checked one.

`manifest.json` lists every pack the image ships: its hash, version,
licence and plan hash; where it ships (`rootfs` for the profile's
store, `initrd`, `installer` for the install initramfs's own tools,
`esp`); and its sources, each with its sha256 and its signing key
(`pkgs/keys/…`), its Sigstore signer or why it is unsigned, or, for a
first-party tree (`apps/…`), its commit. `recipes` gives the plan hash
of each recipe the profile seeds. For its sources, `pkg:oath` records
the commit of this tree instead, with `-dirty` when its sources had
uncommitted changes.

A profile can pin a package Oath does not build (R28): its entry has
`hash` and `needs` (and `url` if it is not on the profile's origins),
and there is no `pkgs/<name>/`:

```json
"pkg:hello": {
  "present": true,
  "hash": "sha256-…",
  "needs": [{ "id": "pkg:glibc", "hash": "sha256-…" }]
}
```

When the profile ships it (`present`), `build` fetches it into
`~/.local/share/oath/store` and the manifest marks its entry
`"pinned": true`; the manifest's `pinned` lists every package the
profile pins this way, shipped or not. When a need's hash is not the one the image
ships, the manifest lists it under `stale_needs`, the image takes the
shipped hash, and `build` and `check` print a warning: its owner should
build it again. `publish` leaves it out. `oath update` moves a machine
to the output's pin and needs, unless a head wins
([Packages](packages.md)), and names an overlay it replaces;
`--keep pkg:<name>` keeps the machine's pin and moves only its needs.

## Test it

```sh
cargo make check              # every QEMU case for the output's profile
cargo make check --build      # pack profile base first
cargo make check --case index --case pkg-fetch
```

Each case boots its own VM from the latest output's `oath.qcow2`
(`--output <id>` for another; four at a time, or `-j <n>`) and runs its
steps over SSH as `home`. The image is the one `install` ships; a case
makes any fixture it needs with `oath new`. A case with `profile` runs
only on that seed profile's image (on a machine, the largest profile
whose objects it has), and one with `qemu = false` runs only with
`--target`. The summary marks each case PASS, FAIL, GAP (a known
failure; the `gap` line names the phase that fixes it) or FIXED (a gap
that passes, which fails the run until its `gap` line is removed). Logs
and the serial console of every boot are under
`build/check/<time>/<case>/`, and `report.json` is in
`build/check/<time>/`. A clean full run writes `check-pass` into the
output it booted, which `cargo make install` requires. Any other
finished run of the cases in QEMU removes it, a passing `--case` run
included.

```sh
cargo make check --target home@10.0.0.108
```

`--target` runs the cases marked `metal = true` on an installed machine
instead: one at a time, as `home`, trusting only the host key that
`install --target` pinned in `build/known_hosts`. A machine installed
any other way has no pin there, and the run stops. The machine must
start in-sync (`oath diff`). After each case the harness runs `oath undo`
back to the generation the case started from and requires `oath diff`
to say `in-sync` again; if it does not, the run stops there. A reboot
step waits up to five minutes for a new boot id. A case that `needs`
hardware (`needs = { what = "a connected display", run = "…" }`) runs
that test first and is `SKIP`ped on a machine that fails it: the
desk's display and Sola cases on a headless machine, the RADV case
without an AMD GPU. QEMU does not run that test: the `desk` case has a
connected display there, and the Sola and RADV cases are `qemu = false`,
so they run only with `--target`. Logs are under
`build/check/<time>-<host>/`. No stamp is written.

### A real model

```sh
ANTHROPIC_API_KEY=… cargo make check --agent              # every task, haiku
cargo make check --agent --case rename --model sonnet
cargo make check --agent --target home@<host>             # on a machine
```

`--agent` gives a model that has never seen Oath one MCP server, the
machine's `ssh -T home@… oath mcp`, and each task in `check/agent.toml`
(rename the machine, run a service, undo a mistake, fix a failing
service, reboot), then checks the machine over SSH. The client is
Claude Code, headless, with no shell, files, memory or `CLAUDE.md`; it
is a template in `check/agent.toml`, so another client can run the same
tasks. A task fails if its checks fail, if a change in the apply log did
not come through MCP, or if the model passed `confirm` where the task
did not ask for it. It is opt-in and never gates `install`: it costs
money (the budget per task is in the file) and a model is not
deterministic. The key is `ANTHROPIC_API_KEY` from the environment; it
refuses to run without one and writes it nowhere. Results are in
`build/out/<id>/agent-check.json` (task, model, pass, cost, turns; a
run of some tasks replaces only their entries),
transcripts in `build/out/<id>/agent/<task>.jsonl`, and each VM's
serial console and steps under `build/check/<time>-agent/<task>/`; with
`--target`, all of it is under `build/check/<time>-<host>-agent/`.

## Run it in QEMU

```sh
cargo make run            # interactive serial on this terminal
cargo make run --build    # pack the base profile first (same flag on up / start)
cargo make up             # serial in the run's log, foreground; Ctrl-C kills QEMU
cargo make start          # serial in the run's log, background
cargo make ssh            # ssh -p 2222 home@127.0.0.1
cargo make stop
```

Each run boots `build/out/latest` and gets `build/runs/<time>-<mode>/`
(`int`, `up` or `vm`) with a qcow2 overlay (the built image is never
changed), `serial.log`, and `meta.json`. Every mode passes the
build host's SSH public keys to the guest. QEMU uses KVM when
`/dev/kvm` is there, 4 GiB of RAM, virtio disk, net, and rng.

The serial console is a **root** shell; SSH is **home**. Then:

```
oath
oath ls
```

When `DISPLAY` is set, or `OATH_DISPLAY` is anything but `none` or `0`,
`run`, `up` and `start` open a gtk window on the guest's virtio-gpu: its
screen, on `base` a shell as `home` (`svc:screen`), 1280 by 800
(`OATH_DISPLAY_WIDTH`, `OATH_DISPLAY_HEIGHT`, each 640 to 7680).
`OATH_DISPLAY=none` or `0` hides the window. `pkg:qemu` has no gtk: an
Oath build host with `DISPLAY` set needs `OATH_DISPLAY=none`, or QEMU
refuses to start.

## Leave the VM

Ctrl-D leaves the **shell**; `svc:serial` is `restart=always`, so PID 1
starts a new one. There is no QEMU monitor, and Ctrl-C goes to the
guest under `run`. To stop the VM, power it off from inside:

```
sudo poweroff
```

(or `sudo oath set host:local power=halt` and
`sudo oath apply --confirm`). PID 1 stops the services, syncs and
remounts `/` read-only first.

`run` also exits when the guest reboots (`-no-reboot`). If the prompt
is dead, `pkill -f qemu-system-x86_64` from another terminal.

## Environment

| Variable | Effect on `cargo make` |
|----------|------------------------|
| `OATH_BUILD` | Directory for `out/`, `runs/`, `check/`, `release/` and `known_hosts` (default `build/`). `fetch/`, `pkgs/`, `bootstrap/`, `audit/` and `review/` stay in the tree's `build/`. |
| `OATH_STORE` | The host store. Default `$XDG_DATA_HOME/oath/store`, else `~/.local/share/oath/store`. |
| `OATH_KERNEL` | A kernel to pack in place of `pkg:linux`'s, and to boot in place of the output's `bzImage` (`run`, `up`, `start`, `check`). |
| `OATH_MODULES` | A modules directory to pack in place of `pkg:linux`'s. |
| `OATH_IMAGE` | A qcow2 to boot in place of the output's `oath.qcow2` (`run`, `up`, `start`, `check`). |
| `OATH_SSH_PORT` | Host port forwarded to the guest's 22 (default 2222): `run`, `up`, `start`, `ssh`, and the QEMU steps of `install` and `update`. `check` takes 2300 and up, `check --agent` 2400 and up. |
| `OATH_SSH_PUBKEY` | A file of public keys, added to the ones [Network](network.md) lists. |
| `OATH_BRIDGE` | A host bridge in place of the port forward ([Network](network.md)): `run`, `up`, `start`, and `check`, whose cases then cannot reach their VMs. |
| `DISPLAY`, `OATH_DISPLAY` | Whether `run`, `up` and `start` open a window: they do when `DISPLAY` is set. `OATH_DISPLAY=none` or `0` hides it; any other value opens it without `DISPLAY`. |
| `OATH_DISPLAY_WIDTH`, `OATH_DISPLAY_HEIGHT` | The guest's screen, 640 to 7680 (default 1280 by 800). |
| `OATH_SEED_DIR` | The seed profiles (default this tree's `crates/oath-core/seed`). |
| `ANTHROPIC_API_KEY` | `check --agent`'s key. |
| `OATH_SIGN_KEY`, `OATH_ORIGIN_S3`, `OATH_ORIGIN_KEY_ID`, `OATH_ORIGIN_SECRET`, `XDG_CONFIG_HOME` | Signing and the origin's write key ([Packages](packages.md)). |
| `OATH_OSV` | `advisories`' OSV query URL (default `https://api.osv.dev/v1/query`). |
| `OATH_CVELIST`, `NVD_API_KEY` | `audit`'s CVE List mirror (default `~/.local/share/oath/cvelist`) and NVD key (else `nvd.env` beside `origin.env`). |

## Telemetry

PID 1 and `oath` write JSON lines prefixed `oath-tel ` to stderr, and
to `/oath/log/*.jsonl` once the disk is mounted.
