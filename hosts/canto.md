# canto

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Mac Pro (late 2013). Dual AMD Pitcairn (FirePro,
  `1002:6810`, GFX6/SI; needs `amdgpu.si_support=1` and the Pitcairn
  firmware). Intel HDA (Cirrus) + HDMI audio. Broadcom BCM20702
  Bluetooth. Two tg3 Ethernet ports. Disk `/dev/sda` (250 GB; GPT:
  512 MiB ESP + btrfs `@`). No serial port.
- **Network:** the cable is in the first port (`00:3e:e1:cb:06:08`),
  DHCP 10.0.0.3, which novus's `/etc/hosts` calls `canto`. The lease's
  DNS (10.0.0.146, then 1.1.1.1) answers `novus` with 10.0.0.5, so
  `host:local.hosts` is empty. The second
  port (`00:3e:e1:cb:04:f0`) has no cable. PID 1 picks the NIC with
  carrier at boot; moving the cable needs a reboot. Reach:
  `ssh home@10.0.0.3`, host key pinned in `build/known_hosts`.
- **Overlay:** [canto.json](canto.json) — hostname, Mountain time,
  nightly `btrfs send` to the NAS at `10.0.0.12:/mnt/alpha/backup/canto`.
- **Permissions:** the test machine. Agents install, update, reboot
  and wipe it without asking (Joshua, 2026-10-18).
- **Running now:** build output `79301d6ea406` (`cargo make build
  --profile desk`: Sola's packs from its `next` at `c6cda58b`), the
  `desk` profile (R20–R22: base plus seatd, rivulet, Sola with its
  terminal, Projects, browser, Design and KVM, PipeWire, Noto, Vulkan,
  the tools that build Sola, and their packages), reached on 2026-10-23
  by `cargo make update --target home@10.0.0.3 --confirm` (generation
  268: `pkg:oath`; Sola's dev pins kept). PID 1 is `4ec147de8bb5`'s,
  from the reboot of 2026-10-21 15:52Z (`build/runs/canto-reboot.sh` on
  novus); the next reboot runs `79301d6ea406`'s. The system was
  `cargo make install --target home@10.0.0.3 --disk /dev/sda
  --hostname canto --overlay hosts/canto.json --confirm` on
  2026-10-18. Passed all 42 `metal` cases (`cargo make check --target
  home@10.0.0.3`, 2026-10-20) on `f897ffcbda55`; on `b845739dac07`,
  `desk-sola` passes, Design included (the full run was not repeated).
- **Steam (R29):** `pkg:glibc32`, `pkg:libxcrypt` and `pkg:bubblewrap`
  went in by `oath store` from novus's packs on 2026-10-23 (generation
  267, the hashes the lock has); `cargo make update` left
  `pkg:steam-src`, `plan:steam` and `pkg:steam` to the machine, made as
  the manual says (269, 270). `oath build plan:steam` made novus's pack
  (`sha256-4302fc2c…`); `/bin/steam` runs Valve's 64-bit client
  (publicbeta, client 1792711043) to its login screen on DP-10, its
  webhelper on radeonsi through ANGLE; not signed in. Steam's data is
  `/home/.local/share/Steam`. Both FirePros show in Vulkan as RADV
  PITCAIRN (1.3.354). `pkg:glibc32` is `glibc-i386` from the next build
  on; the 32-bit Mesa and the pad drivers come with it and a reboot.
- **Design (R21):** `pkg:sola-design` came after the install, so it was
  made by hand on 2026-10-20 from the image's seed values (`oath new
  pkg:sola-design --from-json`, generation 200). Design is open and in
  Sola's saved session; a test canvas rendered its board through CEF.
  Sharing needs `cloudflared`, which canto does not have.
- **Session restore:** `svc:sola-tmux` and `svc:sola-tmux-ws` were made
  by hand on 2026-10-20 as the `desk` seed has them (generations 195 and
  196: `svc:sola-session` stopped, the two made, the session started
  wanting them). Restarting `svc:sola-session` kept the shells (same
  pids, scrollback); after a reboot the terminal's tab came back in
  `/home/Projects` and Projects' pane in `/home/Projects/Sola`.
- **Shell (R27):** `home` logs in to zsh. The update made `pkg:zsh`
  (generation 164); the six `svc:sola-*` were set to `SHELL=/bin/zsh`
  by hand (generation 165), as the `desk` seed now has them, so the
  terminal's tmux starts `-zsh`. The prompt's colors are `#8D3CC3`
  (`home@canto`) and `#6F2A9D` (the branch).
- **Origins (R25):** `host:local.origins` is
  `["https://store.oath.moonlight.pm"]`, set by hand on 2026-10-19
  (generation 135) because the install predates the field; a fresh
  install gets it from the `base` profile.
- **dig (R24):** `pkg:bind`, `pkg:openssl`, `pkg:libuv`,
  `pkg:liburcu` and `pkg:libcap` came after the install: `cargo make
  update` shipped their packs, and their objects were made by hand on
  2026-10-19 (`oath new … --from-json` with the image's seed values,
  generation 108).
- **Claude Code:** the recipe (R23) came after the install, so its
  objects were made by hand on 2026-10-19 as `cargo make update`
  suggests (`oath set plan:claude-code --from-file`, `oath new
  pkg:claude-code-src`, `oath new pkg:claude-code`, the values the
  image seeds). `oath build plan:claude-code` made the pack novus made
  (`sha256-cb973b8f…`), and `pkg:claude-code` is present: `claude`
  2.1.292. Not signed in yet (`claude`, then `/login`). The base system before it
  was `218fd2e1e03b` (GCC and glibc, R17, R18).
- **Backup:** `backup-send` mounts the NAS export as NFS 4.0
  (`-t nfs -o vers=4,addr=…`; the kernel refuses `-t nfs4` with no
  version). Mounting and unmounting it was checked on 2026-10-18; the
  first send from this system is the 04:00 run.
- **SSH to novus:** `home` has an ed25519 key (`/home/.ssh/id_ed25519`,
  `home@canto`, made 2026-10-20) that novus's `joshua` accepts from
  10.0.0.3 only (`from="10.0.0.3"` in `~joshua/.ssh/authorized_keys`);
  `/home/.ssh/config` names `joshua` for `novus`, so `ssh novus` logs
  in. The key lives in `/home`, outside the catalog: undo and update
  leave it, a reinstall loses it.
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
- **Vulkan:** both GPUs are RADV devices (`AMD Radeon R9 200 Series
  (RADV PITCAIRN)`, Vulkan 1.3). The loader alone lists `02:00.0`
  first; the device-select layer puts `06:00.0`, rivulet's, first for
  a Wayland client (it asks rivulet) and for any other (the boot VGA
  card). A program with CEF's `Release` in its RUNPATH loads
  `/lib/libvulkan.so.1` too, since `pkg:cef` dropped Chromium's: all
  of Sola's apps, the browser and Design included, draw with RADV on
  `06:00.0` (the Sola tab, 2026-10-22).
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
  build` of Sola's session crates from a fresh clone took 5 min 45 s
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
