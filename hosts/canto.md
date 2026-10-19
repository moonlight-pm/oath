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
- **Permissions:** the test machine. Agents install, update, reboot
  and wipe it without asking (Joshua, 2026-10-18).
- **Running now:** build output `6783cb7d7f13`, the `desk` profile
  (R20–R22: base plus seatd, rivulet, Sola with its terminal,
  Workspaces, browser and KVM, PipeWire, Noto, the tools that build
  Sola, and their packages), reached on 2026-10-19 by `cargo make
  update --target home@10.0.0.3 --confirm` (generation 27) and a
  reboot, from `b5a58ee98753`, which `cargo make install --target
  home@10.0.0.3 --disk /dev/sda --hostname canto --overlay
  hosts/canto.json --confirm` wrote on 2026-10-18. Passes all 39
  `metal` cases (`cargo make check --target home@10.0.0.3`), `desk`,
  `desk-sola` and `desk-sound` among them. The base system before it
  was `218fd2e1e03b` (GCC and glibc, R17, R18).
- **Backup:** `backup-send` mounts the NAS export as NFS 4.0
  (`-t nfs -o vers=4,addr=…`; the kernel refuses `-t nfs4` with no
  version). Mounting and unmounting it was checked on 2026-10-18; the
  first send from this system is the 04:00 run.
- **Agent:** `ssh -T home@10.0.0.3 oath mcp` serves the verbs as MCP
  tools. All five real-model tasks passed here with haiku on
  `e3a11645f14c` (2026-10-17); not run since the reinstall.
- **Screen:** the monitor is on DP-10 of the GPU at `06:00.0`, which
  is `/dev/dri/card1` and wlroots' primary GPU. The Apple logo stays
  until amdgpu loads (about 8 s after `svc:rivulet` starts, which waits
  for it), then rivulet modesets DP-10 and renders with radeonsi on
  ACO (`pitcairn, ACO`), both GPUs open; libinput has the Razer Taipan
  and the Apple Magic Keyboard through seatd. Sola's menubar is up;
  `solactl session launch sola-terminal` or `sola-browser` opens an
  app (`XDG_RUNTIME_DIR=/run/user/1`), and `solactl compositor
  screenshot -o <png>` captures the screen. `svc:screen` is off.
- **Sound:** PipeWire's sinks are the Intel HDA analog output (the
  default; `alsa_output._sys_devices_pci0000_00_0000_00_1b.0_…`, named by
  sysfs path because libudev-zero has no `ID_PATH`) and the monitor's
  HDMI/DP audio on the GPU at `06:00.1`; the other GPU's HDMI, with
  nothing attached, has none. The shell's volume menu switches them.
- **KVM:** novus's `sola-kvm server --input evdev` (its
  `~/.config/sola-kvm/config.toml`: canto at 10.0.0.3:4242, to the
  right) reaches `svc:sola-kvm` here; its clipboard channel connects at
  boot.
- **Building Sola here:** a login shell has `PKG_CONFIG_PATH`; `cargo
  build` of oath-sola's session crates from a fresh clone took 5 min 45 s
  (debug, 8 threads) on 2026-10-18.
- **Modules:** at boot ahci, sd_mod, tg3, xhci_pci, ehci_pci, usbhid,
  hid_apple, hid_generic, btusb, i2c_i801, evdev, x86_pkg_temp_thermal; after
  services start amdgpu (both GPUs) and snd_hda_intel with the Cirrus
  and ATI HDMI codecs.
- **Firmware:** Apple firmware boots `EFI/BOOT/BOOTX64.EFI` (oath-efi)
  on a freshly formatted ESP with no help; the menu is behind the logo.
  A reboot takes about a minute. SI DPM driver patches are in
  `pkgs/linux/patches/`.
- **Known noise:** `svc:serial` runs and idles (no UART; it logs
  `no serial console`), and there is no `dev:ttyS0`. amdgpu logs a
  missing `amdgpu/vce_1_0_0.bin` (video encode; not packed). dhcpcd's
  wireless probe loads `cfg80211`, which logs a missing
  `regulatory.db`. The firmware leaves Thunderbolt bridge windows
  unassigned (`failed to assign` at boot). The RTC matched NTP on
  2026-10-18; chronyd steps the clock if it drifts.
