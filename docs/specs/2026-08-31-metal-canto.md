**Date:** 2026-08-31
**Status:** target (freeze)
**Implementation:** not started
**Dogfood:** none — QEMU probe stays; canto is the first metal
**Gaps:** installer ramdisk, kexec host CLI, EFI bootloader, real
block/NIC modules, QEMU-EFI rehearsal, canto wipe
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Metal canary: replace canto (`nixos-anywhere` shape)

D1 was QEMU only, no bare-metal installer. That hold lifts for **one
named machine**. Canto already runs a Linux we can SSH into. We fully
replace that OS with Oath. QEMU remains the default probe.

This is **not** nixos-anywhere, not flakes, not disko, not NixOS.
The *protocol* is the same: SSH to a live Linux, kexec an installer
in RAM, format, copy the system, install EFI, reboot.

---

## Locks this freeze owns

- **One machine: canto.** Destructive. The existing OS is gone. Sola’s
  `ssh canto` deploy desk is gone (T24 still says develop Sola on Nix
  until Oath is the host — that Nix box is no longer canto).
- **QEMU probe stays.** Metal is extra dogfood, not a replacement for
  `cargo make probe`.
- **Host verb:** `cargo make install --target root@<host> --disk <node>
  --confirm`. The disk node is the whole disk we wipe (GPT). Without
  `--confirm` the host CLI refuses (catalog `confirm` class). Do not
  write a disk the operator did not name.
- **Phases** (same order as nixos-anywhere):

  1. **kexec** — SSH to the live OS; upload kernel + **installer**
     initrd; `kexec -e`. After kexec the old userspace is dead.
  2. **format** — GPT: ESP (FAT32) + rest **btrfs**; subvolume `@`.
     Same layout as the appliance (`@` live, `@gen-N` siblings).
  3. **install** — copy the packed root tree into `@`; write EFI;
     inject owner **pubkeys** into `ssh:local` (same as `up`/`start`).
  4. **reboot** — firmware boots ESP → our kernel+initrd → mount `@`.

- **Installer ramdisk is not the appliance initrd.** Same kernel.
  Extra modules: NVMe, AHCI, common Ethernet, plus the virtio set so
  QEMU can rehearse. In install mode PID 1 does **not** `switch_root`;
  it starts dropbear and waits. The host SSHes into that ramdisk for
  format+copy.
- **EFI: systemd-boot the binary only** (borrowed prebuilt, D8). One
  loader entry. Not systemd. No generation picker (still the open
  technical question). Undo after first boot is catalog `oath undo`,
  not a firmware menu.
- **After boot:** `host:local` hostname `canto`; `net:*` **dhcp** on
  the real NIC; `dev:*` inventory is real nodes (not `vda`). Serial if
  the box has it; SSH is how the agent reaches it. River/Sola stay
  software GL until a later freeze says otherwise.
- **Rehearse in QEMU with OVMF** before canto. Same CLI, a virtio
  disk, `--confirm`. Courage on metal is SSH after reboot, not a
  gtk window.

Do not add a kind. Install is a **host** action that writes a disk;
the running system is still the catalog.

---

## Courage test (this slice)

**QEMU-EFI rehearsal (required before canto):**

1. `cargo make install --target … --disk /dev/vda --confirm` against
   an OVMF VM that booted some Linux (or the installer ramdisk
   directly). After reboot, SSH with the injected key.
2. `oath ls` works. `pgrep -x sola` empty. Serial if present.

**Canto (operator present):**

1. Operator names the disk node (likely `/dev/nvme0n1` or
   `/dev/sda`). `--confirm`.
2. After reboot: SSH as root with the owner pubkey; `oath get
   host:local` name `canto`; `oath get net:net0 --actual` has dhcp
   lease; old OS is gone.

---

## Out

- nixos-anywhere, disko, flakes, NixOS on the disk
- USB/ISO as the primary path (recovery stick later)
- Dual-boot, preserving canto’s old root
- Boot-generation firmware menu
- General “install any PC” product
- Writing a disk this repo did not name, or without `--confirm`
