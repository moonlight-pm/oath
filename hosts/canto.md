# canto

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Mac Pro (late 2013). Dual AMD Pitcairn (FirePro,
  `1002:6810`, GFX6/SI; needs `amdgpu.si_support=1` and the Pitcairn
  firmware). Intel HDA (Cirrus) + HDMI audio. Broadcom BCM20702
  Bluetooth. Two tg3 Ethernet ports. Disk `/dev/sda` (250 GB; GPT:
  512 MiB ESP + btrfs `@`). No serial port.
- **USB disk:** Seagate BarracudaFastSSD (2 TB, `0bc2:aa16`, USB 3 at
  5 Gb/s), wiped 2026-10-23 to GPT with one btrfs partition, label
  `warehouse`. It is on `uas` since the boot of 2026-10-24 04:31Z.
  `host:local.mounts` (generation 277) mounts it at `/mnt/warehouse`
  (`noatime,compress=zstd:1`); PID 1 has mounted it at boot since
  then (2026-10-24 04:32Z, 2026-10-25 22:20Z). It holds the cargo target dirs of Sola and Oath
  (`~/Projects/{Sola,Oath}/target` link to
  `/mnt/warehouse/{Sola,Oath}/target`), so builds fail while it is not
  mounted. Nothing the machine boots or runs needs it.
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
- **Updates (R38):** canto pulls its own from the bucket. Its
  `host:local` names `profile=desk` and buccina's key in `publishers`,
  set by hand with the newer `pkg:oath` on 2026-10-28 (generations
  374–375), as [install.md](../docs/manual/install.md) says for a
  machine from before R38. The first pull, of `3b2d49886ff7`, stopped at
  `pkg:perl`: canto's link to Cloudflare reset that 99 MB fetch twice
  (novus fetched it in 2 s), leaving the apply half done (376). `oath
  apply` finished it (378); a second `oath update --confirm` wrote the
  root files and the ESP, and the pre-update `@boot-28` was put back by
  hand after it. Its later pulls the same day were ordinary:
  `1fa3b09a85ca` and `e433824d38d0` (no package but `pkg:oath` moved).
  The pull of `0c6eebe617d2` hit the pre-download bug (its `oath`
  pinned `pkg:oath` as it fetched it, so the apply left it unlinked);
  set back and forward by hand (383–384), and `e433824d38d0`'s `oath`
  (`sha256-4eec034d2bb9…`) was taken by hand first (386). `pkg:cef` is
  Oath's CEF 156 with H.264 and AAC (R40, `sha256-2a2cc750`) since
  generation 398, fetched from the bucket, under Sola's own applies of
  its packs from `next` `83df4558` (2026-10-29). Its Oath checkout (`~/Projects/Oath`, a clean
  `d8817f9`; cargo's target on `/mnt/warehouse/Oath`) goes once the
  Steam session in it closes (Sola restored it after the reboot); the
  novus trial stick's host key moved to `~/.ssh/novus-oath.known_hosts`.
- **Running now:** output `e433824d38d0` (pulled 2026-10-28 22:35Z),
  booted 22:48Z with its PID 1 `b922d6d8b855`; every service runs.
  `svc:pipewire` and `svc:pipewire-pulse` have `rtprio` 95 (generation
  385, by hand): PipeWire's data thread is `SCHED_FIFO` and module-rt
  no longer warns. `3b2d49886ff7` (the 2026-10-27 security sweep, Linux
  7.3, OpenSSH, account `void`) booted 2026-10-28 21:41Z. `oath update --confirm` after that boot
  moved `svc:sshd` from dropbear to OpenSSH (generation 380, kept from
  a new session; dropbear's host key carried over). The Sola tab's pins
  of `pkg:sola`, `sola-browser` and `sola-design` stayed. Every service
  runs; amdgpu, HDA and `/mnt/warehouse` came up; `/dev/kvm` is
  root:home 0660. Its root is still named `oath.root=/dev/sda2`. Self-host ran here
  until 2026-10-26 (it stopped at a download, glibc without bison, then
  glibc-i386's GCC with the seed's MPFR-less gawk) and moved to buccina;
  its build catalog (`~/.local/share/oath`), `build/bootstrap`,
  `build/pkgs`, `build/fetch` and logs were deleted that day, and the
  `@gen-N` snapshots of 2026-10-25 still hold their blocks. canto is
  Joshua's Sola R&D desk. The `desk` profile: base plus seatd, rivulet, Sola with its
  terminal, Projects, browser, Design and KVM, PipeWire, Noto, Vulkan,
  the tools that build Sola, Steam, and their packages.
  The system was `cargo make install
  --target home@10.0.0.3 --disk /dev/sda --hostname canto --overlay
  hosts/canto.json --confirm` on 2026-10-18. Passed all 42 `metal`
  cases (`cargo make check --target home@10.0.0.3`, 2026-10-20) on
  `f897ffcbda55`; on `b845739dac07`, `desk-sola` passes, Design
  included (the full run was not repeated).
- **Steam (R29):** made by hand after the install, as the manual says:
  `pkg:steam-src`, `plan:steam` and `pkg:steam` (generations 269, 270;
  `oath build plan:steam` made novus's pack, `sha256-4302fc2c…`), then
  the 17 `-i386` packs `cargo make update` left to the machine (272,
  `oath new` with the output's documents), `pkg:steam`'s needs moved to
  them (273) and `pkg:glibc32`, its first glibc for i386, removed
  (274). `/bin/steam` runs Valve's 64-bit client (publicbeta, client
  1792711043) on DP-10, signed in as Joshua since 2026-10-23 20:46
  (and again by itself after the reboot; restarted on `pkg:steam`
  `a25d01f1…` on 2026-10-24 08:49);
  its webhelper is on radeonsi through ANGLE, and its container imports
  `mesa-i386` (no 32-bit GL or Vulkan errors). No game installed yet.
  Since the boot of 2026-10-25 22:19Z `/dev/uinput` is root:home 0660
  (R30); no pad tried yet.
  Steam's data and its one library are `/home/.local/share/Steam`;
  Steam names the library's drive `/home` (before `a25d01f1…`,
  `/run/host/usr/lib/oath`: see [steam.md](../docs/specs/steam.md)). Both FirePros show in
  Vulkan as RADV PITCAIRN (1.3.354).
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
- **Codex and Grok Build (R31):** their recipes came after the install,
  so their objects were made by hand on 2026-10-24 as the update listed
  them (`oath new pkg:<name>-src` and `pkg:<name> --from-json` with the
  output's documents, `oath set plan:<name> --from-file`). `oath build`
  made novus's packs (`codex` `sha256-37194dc8…`, `grok-build`
  `sha256-14d75ad7…`), present since generation 295. `codex` 0.164.0
  and `grok` 1.0.70 run; neither is signed in (`codex login`, `grok
  login`). `bin/grok` wrote `auto_update = false` to
  `~/.grok/config.toml`; `~/.codex/hooks.json` (2026-10-21) predates it.
- **wl-clipboard:** in `desk` after the install, so on 2026-10-25
  05:00Z novus's pack (`sha256-41f03fdf…`, `cargo make pkgs
  wl-clipboard`) was put here with `oath store`, made with `oath new
  pkg:wl-clipboard --from-json` (needs `pkg:wayland`) and applied
  (generation 319). `wl-copy` and `wl-paste` bind rivulet's
  `ext_data_control_manager_v1`, no popup; a PNG copied and pasted back
  matched byte for byte. Claude Code pastes a Super+Shift+4 screenshot
  (Joshua, 2026-10-26). The Sola tab's pin of `pkg:sola` (`a6751239…`)
  needs it (Sola `bd8d8152`); `desk.json`'s older pin does not yet.
- **Backup:** `backup-send` mounts the NAS export as NFS 4.0
  (`-t nfs -o vers=4,addr=…`; the kernel refuses `-t nfs4` with no
  version). Mounting and unmounting it was checked on 2026-10-18; the
  first send from this system is the 04:00 run.
- **SSH to novus and buccina:** `home` has an ed25519 key
  (`/home/.ssh/id_ed25519`, `home@canto`, made 2026-10-20) that novus's
  `joshua` accepts from 10.0.0.3 only (`from="10.0.0.3"` in
  `~joshua/.ssh/authorized_keys`) and buccina's `ssh:local` lists
  (2026-10-28, Joshua). `/home/.ssh/config` names `joshua` for `novus`
  and 10.0.0.149 for `buccina`, so `ssh novus` and `ssh buccina` log
  in; buccina's host key is pinned in `~/.ssh/known_hosts`. The key
  lives in `/home`, outside the catalog: undo and update leave it, a
  reinstall loses it.
- **SSH from buccina:** `ssh:local` lists `home@buccina` (generation
  387, 2026-10-28, Joshua), beside Joshua's `root@hub`.
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
