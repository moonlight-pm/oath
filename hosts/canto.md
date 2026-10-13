# canto

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Mac Pro (late 2013). Dual AMD Pitcairn (FirePro,
  `1002:6810`, GFX6/SI; needs `amdgpu.si_support=1` and the Pitcairn
  firmware in the initrd). Intel HDA (Cirrus) + HDMI audio. Broadcom
  BCM20702 Bluetooth. tg3 Ethernet. Disk `/dev/sda` (GPT: ESP + btrfs).
- **Network:** DHCP, 10.0.0.3. `ssh home@canto`.
- **Overlay:** [canto.json](canto.json) — hostname, Mountain time,
  nightly `btrfs send` to the NAS at `10.0.0.12:/mnt/alpha/backup/canto`.
- **Running now:** the 2026-09 desk image (River + Sola, kernel
  7.3.0-rc1 #4, ESP last-5 menu). Not built from this tree any more.
- **Firmware quirk:** Apple firmware boots first; the `oath-efi` menu is
  behind the logo. Driver patches for SI DPM are in
  `image/linux-patches/`.
- **Next:** wipe and reinstall from the native base (roadmap phase 6):
  `cargo make install --target home@canto --disk /dev/sda --hostname canto --overlay hosts/canto.json --confirm`
  — only when Joshua asks.
