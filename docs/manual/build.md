# Build and QEMU

Everything is built on the build host with `cargo make` (crate
`oath-make`), from a plain shell.

Built here, from source: the kernel; the toolchain (clang, lld, musl,
libc++); every package in the image (`pkgs/`); PID 1 (`oath-init`,
`serial-login`, `sudo`), `oath`, and the UEFI loader `oath-efi`
(`pkg:oath`, built by the vendor's Rust release, `pkg:rust`). The only
things taken from Nix are QEMU and its UEFI firmware for running images
on the build host (`image/host.nix`), and the host compilers that build
stage 0 of the toolchain and `oath-make` itself. None of it ships.

## The toolchain and packages

```sh
cargo make bootstrap        # once: stage 0 + stage 1 toolchain (an hour or more)
cargo make pkgs             # every package, in dependency order
cargo make pkgs --list      # versions, licences, what pkgs/lock.json pins
cargo make pkgs curl        # one package (and what it builds with)
```

`bootstrap` builds clang/lld targeting musl with the host's gcc (stage
0), then a static musl toolchain, busybox and make with stage 0 (stage
1): `pkg:llvm`, `pkg:busybox`, `pkg:make`. `pkgs` builds each
`pkgs/<name>/` as an `oath build` plan in the sandbox (no network, only
the declared packs), writes the exact plan to `pkgs/<name>/<name>.plan`
and the product to `pkgs/lock.json`. `pkg:llvm` is one of them: it
rebuilds itself with the `cmake`, `ninja` and `python` packs, which
are build-only and not in the image. `pkg:rust` is the vendor's Rust
release (pinned by sha256) with its own musl loader; it builds
`pkg:oath`. The image takes every userspace package from the lock. How
packages are written: `pkgs/README.md`.

## The kernel

There is one kernel, `pkg:linux`: vanilla kernel.org Linux, pinned by
sha256 in `pkgs/linux/pkg.toml`, plus `pkgs/linux/patches/` and the
config fragment `pkgs/linux/linux.fragment` over `defconfig`. It is a
plan product like every other package, built by `pkg:llvm`
(`LLVM=1`) with the `flex`, `bison`, `m4`, `perl` and `elfutils`
(libelf, for objtool) packs, which are build-only.

```sh
cargo make pkgs linux
```

The product holds `boot/vmlinuz`, `boot/config` and
`lib/modules/<release>/` with the depmod index (`modules.dep`,
`modules.alias`, `modules.softdep`) written from each module's
`.modinfo`. `cargo make` takes it from `pkgs/lock.json` (or
`OATH_KERNEL` / `OATH_MODULES`). Firmware for the initrd comes from
`pkg:firmware`, not the kernel build. Packing refuses a bzImage whose
release does not match the module directory or the `vermagic` of
`btrfs.ko`. Bump the version and the sha256 together (`pkgs/llvm` takes
its UAPI headers from the same tarball).

## The image

Needs: the packs in `pkgs/lock.json` (`pkg:rust` among them), `sudo`
for the loop mount, and `mkfs.btrfs` on the host (`shell.nix` has it).
The build host's Rust compiles only `oath-make`.

```sh
cargo make build                  # profile base → build/out/<id>/
```

`build` builds `pkg:oath` from this tree: it packs the workspace and
every crates.io crate in `Cargo.lock` (fetched once into `build/fetch`,
checked against the lock's sha256) and runs `cargo build` with
`pkg:rust` in the sandbox, offline. A tree that has not changed is not
rebuilt; `cargo make pkgs oath` does this step alone. Then `build`
writes the boot initrd and the
installer initrd, stages the root tree, seeds `/oath` from the profile,
packs the profile's packages into the hashed store (also cached under
`.cache/oath/store`), and writes a **build output**:

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
`esp`); and its sources, each with its sha256 and signing key
(`pkgs/keys/…`) or why it is unsigned. `pkg:oath` records the commit
of this tree instead, with `-dirty` when its sources had uncommitted
changes.

## Test it

```sh
cargo make check              # every case in check/cases.toml
cargo make check --build      # pack first
cargo make check --case index --case pkg-fetch
```

Each case boots its own VM from the latest output's `oath.qcow2`
(`--output <id>` for another; four at a time) and runs its steps over
SSH as `home`. The image is the one `install` ships; a case makes any fixture it needs with `oath new`. The summary marks each case PASS,
FAIL, GAP (a known failure; the `gap` line names the phase that fixes
it) or FIXED (a gap that now passes; remove its `gap` line). Logs, the
serial console of every boot and `report.json` are under
`build/check/<time>/<case>/`. A clean full run writes `check-pass` into
the output it booted, which `cargo make install` requires.

```sh
cargo make check --target home@10.0.0.108
```

`--target` runs the cases marked `metal = true` on an installed machine
instead: one at a time, as `home`, trusting only the host key that
`install` pinned in `build/known_hosts`. The machine must start
in-sync (`oath diff`). After each case the harness runs `oath undo`
back to the generation the case started from and requires `oath diff`
to say `in-sync` again; if it does not, the run stops there. A reboot
step waits up to five minutes for a new boot id. Logs are under
`build/check/<time>-<host>/`. No stamp is written.

## Run it in QEMU

```sh
cargo make run            # interactive serial on this terminal
cargo make run --build    # pack first (same flag on up / start)
cargo make up             # headless, foreground; Ctrl-C kills QEMU
cargo make start          # headless, background
cargo make ssh            # ssh -p 2222 home@127.0.0.1
cargo make stop
```

Each run gets `build/runs/<id>/` with a qcow2 overlay (the built image
is never changed), `serial.log`, and `meta.json`. Every mode passes the
build host's SSH public keys to the guest. QEMU uses KVM when
`/dev/kvm` is there, 4 GiB of RAM, virtio disk, net, and rng.

The serial console is a **root** shell; SSH is **home**. Then:

```
oath
oath ls
```

When `DISPLAY` is set, `run` also opens a gtk window on the guest's
virtio-gpu: its screen, a shell as `home` (`svc:screen`).
`OATH_DISPLAY=none` hides the window.

## Leave the VM

Ctrl-D leaves the **shell**; `svc:serial` is `restart=always`, so PID 1
starts a new one. There is no QEMU monitor, and Ctrl-C goes to the
guest under `run`. To stop the VM, power it off from inside:

```
sudo poweroff
```

(or `oath set host:local power=halt` and `oath apply --confirm`). PID 1
stops the services, syncs and remounts `/` read-only first.

`run` also exits when the guest reboots (`-no-reboot`). If the prompt
is dead, `pkill -f qemu-system-x86_64` from another terminal.

## Telemetry

PID 1 and `oath` write JSON lines prefixed `oath-tel ` to stderr, and
to `/oath/log/*.jsonl` once the disk is mounted.
