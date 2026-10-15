# canto

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Mac Pro (late 2013). Dual AMD Pitcairn (FirePro,
  `1002:6810`, GFX6/SI; needs `amdgpu.si_support=1` and the Pitcairn
  firmware). Intel HDA (Cirrus) + HDMI audio. Broadcom BCM20702
  Bluetooth. Two tg3 Ethernet ports. Disk `/dev/sda` (250 GB; GPT:
  512 MiB ESP + btrfs `@`). No serial port.
- **Network:** the cable is in the second port (`00:3e:e1:cb:04:f0`),
  DHCP 10.0.0.108. The first port (`00:3e:e1:cb:06:08`) has no cable;
  novus's `/etc/hosts` still maps `canto` to 10.0.0.3, that port's old
  address. Reach: `ssh home@10.0.0.108` (host key pinned in
  `build/known_hosts` by the install).
- **Overlay:** [canto.json](canto.json) — hostname, Mountain time,
  nightly `btrfs send` to the NAS at `10.0.0.12:/mnt/alpha/backup/canto`.
- **Running now:** the native base, installed 2026-10-14 from this tree
  with `cargo make install --target home@10.0.0.108 --disk /dev/sda
  --hostname canto --overlay hosts/canto.json --confirm`, unattended.
  Kernel 7.3.0-rc1. Passes all 22 `metal` cases
  (`cargo make check --target home@10.0.0.108`, 2026-10-15).
- **Firmware:** Apple firmware boots `EFI/BOOT/BOOTX64.EFI` (oath-efi)
  on a freshly formatted ESP with no help; the menu is behind the logo.
  A reboot takes about a minute. SI DPM driver patches are in
  `image/linux-patches/`.
- **Known noise:** `svc:serial` backs off to `failed` (no UART). amdgpu
  logs a missing `amdgpu/vce_1_0_0.bin` (video encode; not packed).
  Every module in the fixed list loads (virtio, nvme, e1000e, …) until
  modalias loading.
