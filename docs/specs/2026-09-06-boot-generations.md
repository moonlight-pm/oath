**Date:** 2026-09-06
**Status:** target (freeze)
**Implementation:** in tree (picker + `oath.subvol` + ESP rotate + `cargo make esp`)
**Dogfood:** not on canto ESP until `cargo make esp --confirm` (needs packed
  initrd + `oath-efi`). QEMU `run`/`probe` stay `-kernel` (no menu).
**Gaps:** metal timeout menu unsmoked; kernel 7.3 only on the next nix pack;
  River GLES is still the river-pack Mesa until that pack rebuilds
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Boot generations (last 5) + current packages

Greenfield: no compatibility contract. Borrowed prebuilts track **current**
upstream, not an LTS we outgrew. A bad kernel or ESP initrd must be
recoverable from firmware without SSH.

This lifts T27 **Out: Boot-generation firmware menu**. Catalog `@gen-N`
undo is unchanged. These are **boot** slots (kernel + initrd + a root
subvolume), not `oath undo`.

## Locks

- **Five archived boots plus current.** ESP keeps `/vmlinuz` + `/initrd.gz`
  as the default, and `/oath/boot/<id>/{vmlinuz,initrd.gz}` for the last
  five successful ESP writes. Id is a monotonic integer, never reused.
- **`oath-efi` is the menu.** It is `BOOTX64.EFI`. It reads
  `loader/loader.conf` (`timeout`, `default`) and `loader/oath-boots`
  (newest-first list of BLS filenames). Timeout **5** on metal, **0** on
  QEMU EFI rehearsal (probe/run stay `-kernel`). Up/Down/Enter/Esc.
  systemd-boot stays fallback only.
- **`oath.subvol=`** on the kernel command line. PID 1 mounts that btrfs
  subvolume as `/`. Allowed names only: `@`, `@boot-<digits>`,
  `@gen-<digits>`. Anything else is ignored and `@` is used. Missing
  subvolume falls back to `@`.
- **`@boot-N`** is a read-write snapshot of `@` taken when the ESP is
  rotated (so an old kernel boots matching userspace). Catalog
  `@gen-N` remains apply/undo. Skip the snapshot if the top-level is
  not mounted.
- **Non-wipe ESP update:** `cargo make esp --esp /dev/sda1 --confirm`.
  Rotates, copies new `oath-efi` + kernel + initrd, writes BLS.
  Does **not** format a disk. Full install still `--confirm` + `--disk`.
- **Kernel:** newest packaged kernel the build nixpkgs knows
  (`linuxPackages_testing` / `linux_7_3` when present, else
  `linuxPackages_latest`). Pitcairn still sets `amdgpu.si_support=1`
  until the running kernel defaults SI to amdgpu.
- **`pkg:mesa`:** current Debian mesa (26.2.x), not a frozen 26.1.6.
  Gamescope/RADV follow that pack. River GLES follows `pkg:river`
  until river-pack is rebuilt.

## Out

- systemd-boot as the operator menu
- More than five archived ESP boots
- Pairing catalog undo with firmware entries automatically
- Building a kernel tree in this repo (still borrowed)

## Courage

Metal: firmware menu lists current + at least one archived boot after
the first `cargo make esp`. Selecting an archive boots that kernel and
`oath.subvol=@boot-N`. QEMU probe still does not wait on a menu.
