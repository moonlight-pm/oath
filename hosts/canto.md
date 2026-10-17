# canto

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Mac Pro (late 2013). Dual AMD Pitcairn (FirePro,
  `1002:6810`, GFX6/SI; needs `amdgpu.si_support=1` and the Pitcairn
  firmware). Intel HDA (Cirrus) + HDMI audio. Broadcom BCM20702
  Bluetooth. Two tg3 Ethernet ports. Disk `/dev/sda` (250 GB; GPT:
  512 MiB ESP + btrfs `@`). No serial port.
- **Network:** the cable is in the first port (`00:3e:e1:cb:06:08`),
  DHCP 10.0.0.3, which novus's `/etc/hosts` calls `canto`. The second
  port (`00:3e:e1:cb:04:f0`) has no cable. PID 1 picks the NIC with
  carrier at boot; moving the cable needs a reboot. Reach:
  `ssh home@10.0.0.3`, host key pinned in `build/known_hosts`.
- **Overlay:** [canto.json](canto.json) — hostname, Mountain time,
  nightly `btrfs send` to the NAS at `10.0.0.12:/mnt/alpha/backup/canto`.
- **Running now:** build output `e3a11645f14c` (commit `bea6b36`,
  `oath mcp`), reached on 2026-10-17 with `cargo make update --target
  home@10.0.0.3 --output e3a11645f14c --confirm --reboot` from
  `18823a7556d0` (generation 72; the old boot is archived as boot 3).
  Installed 2026-10-16 with `cargo make install --target home@10.0.0.3
  --disk /dev/sda --hostname canto --overlay hosts/canto.json
  --confirm`, unattended (EFI one-shot installer entry). Passes all 37
  `metal` cases (`cargo make check --target home@10.0.0.3`), the five
  `mcp-*` cases among them. SI DPM initializes on both GPUs.
- **Agent:** `ssh -T home@10.0.0.3 oath mcp` serves the verbs as MCP
  tools. All five real-model tasks pass here with haiku (`cargo make
  check --agent --target home@10.0.0.3`, 2026-10-17).
- **Screen:** the monitor is on DP-10 of the GPU at `06:00.0`. The
  Apple logo stays until amdgpu loads (about 10 s), then fbcon
  (1920×1080, 240×67) shows `svc:screen`'s `home` shell on tty1.
  `svc:screen` was added with `oath new` (generation 45): updates do
  not create seed objects.
- **Modules:** at boot ahci, sd_mod, tg3, xhci_pci, ehci_pci, usbhid,
  hid_apple, hid_generic, btusb, i2c_i801, x86_pkg_temp_thermal; after
  services start amdgpu (both GPUs) and snd_hda_intel with the Cirrus
  and ATI HDMI codecs.
- **Firmware:** Apple firmware boots `EFI/BOOT/BOOTX64.EFI` (oath-efi)
  on a freshly formatted ESP with no help; the menu is behind the logo.
  A reboot takes about a minute. SI DPM driver patches are in
  `pkgs/linux/patches/`.
- **Known noise:** `svc:serial` runs and idles (no UART; it logs
  `no serial console`), and there is no `dev:ttyS0`. amdgpu logs a missing `amdgpu/vce_1_0_0.bin`
  (video encode; not packed). The RTC runs about 100 s fast; `svc:time`
  steps it back at every boot.
