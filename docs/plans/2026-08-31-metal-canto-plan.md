# Plan — Metal canary (canto)

**Date:** 2026-08-31
**Status:** open
**Proof:** (QEMU-EFI rehearsal, then canto)
**Freeze:** [../specs/2026-08-31-metal-canto.md](../specs/2026-08-31-metal-canto.md)

- [ ] T27 freeze: canto; SSH+kexec; not nixos-anywhere; QEMU probe stays.
- [ ] Installer initrd: NVMe/AHCI/NIC modules; `oath.install=1` (no
      switch_root; dropbear; wait).
- [ ] Host `cargo make install --target --disk --confirm`: kexec,
      format GPT+ESP+btrfs `@`, copy packed tree, systemd-boot EFI,
      inject pubkeys, reboot.
- [ ] QEMU OVMF rehearsal. Probe subset over SSH.
- [ ] Canto wipe (operator names the disk). SSH courage. Manual.
