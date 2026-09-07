**Date:** 2026-09-06
**Status:** target (freeze)
**Implementation:** in tree + on canto ESP (picker + `oath.subvol` + ESP rotate + `cargo make esp`)
**Dogfood:** Ubuntu 7.3-rc1 panicked; rescue **boot 4** (6.12.93).
  Live canto is **7.3.0-rc1 #3** (DPM timings + RunningOnAC).
  **boot 7** is previous DPM #2. QEMU `run`/`probe` stay `-kernel`.
**Gaps:** metal timeout menu: systemd-boot is BOOTX64 so the list is
  visible; oath-efi still paints over it. River GLES is still the
  river-pack Mesa; GFX6 modifiers need that River mesa on this kernel.
  `pkg:mesa` live is 26.2.1. SI DPM: `RunningOnAC` OK, pick
  UI_PERFORMANCE, vblank 464µs; clocks still **300/150** (`SetForcedLevels
  1` fails). boot 7 = previous 7.3 DPM #2; boot 1 pruned.
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
- **Kernel:** vanilla **kernel.org** source, **Oath `.config`**, we
  compile (`image/build-linux.sh` + `image/linux.fragment` + optional
  `image/linux-patches/*.patch`). Not Ubuntu generic, not a NixOS
  kernel, not a linux.git fork in this repo. Small Oath patches are
  allowed (SI DPM under Display Core). Same class as packing Zig:
  borrow upstream, ship the bits we need. Version tracks **7.3**
  (GFX6 DRM modifiers for Pitcairn). Drivers are the initrd
  `MODULE_ROOTS` list (amdgpu SI, tg3, btrfs, virtio, HDA, NFS, …)
  plus EFI stub and `IA32_EMULATION` (32-bit Steam). **Pitcairn
  firmware** (`amdgpu/pitcairn_*.bin`) must be in the initrd
  (`OATH_FIRMWARE` or `$OATH_KERNEL` sibling `firmware/`); PID 1
  bind-mounts initrd `/lib/firmware` over `@`. Pitcairn still sets
  `amdgpu.si_support=1` until the running kernel defaults SI to
  amdgpu. QEMU `nix-shell` may still point `OATH_KERNEL` at a nixpkgs
  bzImage until the next image rebuild.
- **`pkg:mesa`:** current Debian mesa (26.2.x), not a frozen 26.1.6.
  Gamescope/RADV follow that pack. River GLES follows `pkg:river`
  until river-pack is rebuilt.

## Out

- systemd-boot as the operator menu
- More than five archived ESP boots
- Pairing catalog undo with firmware entries automatically
- A `linux.git` submodule (vanilla tarball + fragment + `image/linux-patches`)
- Ubuntu/Debian/NixOS packaged generic kernels as the metal kernel

## Courage

Metal: firmware menu lists current + at least one archived boot after
the first `cargo make esp`. Selecting an archive boots that kernel and
`oath.subvol=@boot-N`. QEMU probe still does not wait on a menu.
