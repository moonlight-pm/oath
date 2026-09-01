# Plan — Metal canary (canto)

**Date:** 2026-08-31
**Status:** complete (gaps on freeze)
**Proof:** QEMU-EFI rehearsal `oath ls`; canto SSH as root, `host:local`
canto, `net:net0` dhcp 10.0.0.3. Arch gone.
**Freeze:** [../specs/2026-08-31-metal-canto.md](../specs/2026-08-31-metal-canto.md)

- [x] T27 freeze: canto; SSH+kexec *shape*; not nixos-anywhere; QEMU probe stays.
- [x] Installer initrd: NVMe/AHCI/NIC modules; `oath.install=1` (no
      switch_root; dropbear; wait).
- [x] Host `cargo make install --target --disk --confirm`: enter installer
      (EFI oneshot or USB; kexec on this Apple left tg3 dead), format
      GPT+ESP+btrfs `@`, copy packed tree, systemd-boot EFI, inject
      pubkeys, reboot.
- [x] QEMU OVMF rehearsal. Probe subset over SSH.
- [x] Canto wipe (`/dev/sda`). SSH courage. Manual.
