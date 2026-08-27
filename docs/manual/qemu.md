# QEMU appliance (limited)

**Status:** partial. Serial QEMU only. Not an installer. Not bare metal.

Build tools (kernel, busybox, qemu, musl cc) are borrowed for the host
build. They are not the runtime identity.

## Build and run

Needs: Rust with `x86_64-unknown-linux-musl`, loop-mount as root (`sudo`
for packing the disk), btrfs-progs, qemu.

```sh
nix-shell          # optional: kernel, qemu, musl cc, busybox, btrfs
./image/build.sh
./image/run.sh     # serial on stdio; KVM if /dev/kvm is usable
```

You land on a root shell. The catalog is `/oath`. Try `oath` with no
arguments, then `oath ls`.

## What works here

- Boot to serial, `oath ls` / `get` / `schema` / `set` / `diff` / `apply` / `undo`
- Hostname apply and undo on the live VM
- `oath apply` of `power=reboot` without `--confirm` refuses (exit 3)

## Gaps

- Hostname surviving a **reboot** is not an e2e sign-off yet (init does
  reapply `host:local` on boot).
- No SSH, no installer, no packages.
