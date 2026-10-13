# Build and QEMU

Everything is built on the build host with `cargo make` (crate
`oath-make`). Run it from a plain shell, not `nix-shell`: `shell.nix`
sets `OATH_BUSYBOX`, which skips the `image/tools.nix` build that
supplies dropbear and the other borrowed tools.

Built here: the kernel, PID 1 (`oath-init`, `serial-login`, `sudo`),
`oath`, and the UEFI loader `oath-efi`, all from this repo. Borrowed
from Nix at build time (`nix-build image/tools.nix`), until the native
base replaces them: busybox, btrfs-progs, dropbear and the OpenSSH
client bits, curl and its CA bundle, sgdisk, mkfs.fat, kexec, GNU tar,
systemd-boot (fallback loader), OVMF (QEMU only), and firmware blobs.

## The kernel

There is one kernel: vanilla kernel.org Linux, pinned by sha256 in
`image/build-linux.sh`, plus `image/linux-patches/` and the config
fragment `image/linux.fragment`.

```sh
nix-shell image/linux-build.nix --run 'sh image/build-linux.sh'
```

That writes `build/linux/vmlinuz`, `build/linux/modules/<release>/`,
and `build/linux/firmware/`. `cargo make` reads those (or
`OATH_KERNEL` / `OATH_MODULES` / `OATH_FIRMWARE`). Packing refuses a
bzImage whose release does not match the module directory or the
`vermagic` of `btrfs.ko`. Bump the version and the sha256 together.

## The image

Needs: Rust with `x86_64-unknown-linux-musl` and `x86_64-unknown-uefi`,
`sudo` for the loop mount, `mkfs.btrfs` on the host.

```sh
cargo make build                  # profile base
cargo make boot                   # kernel + initrd + oath-efi only
```

`build` compiles the musl binaries, writes the boot initrd and the
installer initrd, stages the root tree, seeds `/oath` from the profile,
packs the profile's packages into the hashed store (also cached under
`.cache/oath/store`), and writes `build/oath.qcow2` (btrfs, subvolume
`@`) plus `build/bzImage`, `build/initrd.gz`, and
`build/initrd-install.gz`.

## Test it

```sh
cargo make check              # every case in check/cases.toml
cargo make check --build      # pack first
cargo make check --case index --case pkg-hello
```

Each case boots its own VM from `build/oath.qcow2` (four at a time) and
runs its steps over SSH as `home`. The summary marks each case PASS,
FAIL, GAP (a known failure; the `gap` line names the phase that fixes
it) or FIXED (a gap that now passes; remove its `gap` line). Logs, the
serial console of every boot and `report.json` are under
`build/check/<time>/<case>/`. A clean full run writes `build/check-pass`,
which `cargo make install` requires.

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
virtio-gpu. The image has no desktop, so use serial or SSH.
`OATH_DISPLAY=none` hides the window.

## Leave the VM

Ctrl-D leaves the **shell**; `svc:serial` is `restart=always`, so PID 1
starts a new one. There is no QEMU monitor, and Ctrl-C goes to the
guest under `run`. To stop the VM, halt it from inside:

```
oath set host:local power=halt
oath apply --confirm
```

`run` also exits when the guest reboots (`-no-reboot`). If the prompt
is dead, `pkill -f qemu-system-x86_64` from another terminal.

## Telemetry

PID 1 and `oath` write JSON lines prefixed `oath-tel ` to stderr, and
to `/oath/log/*.jsonl` once the disk is mounted.
