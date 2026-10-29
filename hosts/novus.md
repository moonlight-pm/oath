# novus

Overwrite this file when facts change; do not append a diary.

- **Role:** Joshua's daily machine, on NixOS 25.11
  (`/etc/nixos/configuration.nix`). It was Oath's build host until
  2026-10-28, when Oath work moved to buccina (R39); its checkout of
  `moonlight-pm/oath` matches GitHub. It leaves NixOS for its own Oath
  install (R32); until then nothing writes its disks or changes its OS
  without Joshua's word.
- **Hardware:** ASUS ProArt X570-Creator WiFi, Ryzen 9 5950X (16 cores,
  32 threads), 128 GB. NVIDIA RTX 3090 Ti (GA102, `10de:2203`) on a
  custom water loop. Realtek ALC1220 audio and the card's HDMI audio.
  Intel AX210 Wi-Fi and Bluetooth, Thunderbolt 4 (Maple Ridge), Nuvoton
  NCT6798D fan and sensor chip, a USB LifeCam Cinema.
- **Network:** the cable is in the Intel I225-V (`igc`,
  `04:42:1a:e7:35:6f`, at 1 Gb/s), DHCP 10.0.0.5. The Aquantia AQC113
  10 GbE (`atlantic`, `04:42:1a:e7:35:6e`) and the Wi-Fi have no link.
- **Disks:** three SATA SSDs. `sda` (WD Blue, 500 GB): NixOS's root and
  `/nix`. `sdc` (WD Blue, 2 TB): NixOS's ESP and `/home` (btrfs
  `@home`, `@snapshots`). `sdb` (SPCC, 4 TB): `/mnt/storage` (btrfs
  `@data`, `@backups`, the hourly btrbk target).
- **GPU (R34):** on NixOS, NVIDIA 580.142's closed module with GSP off
  (`NVreg_EnableGpuFirmware=0`), capped at 250 W with the core clock
  locked to 210–1500 MHz by `nvidia-smi`, after repeated Xid 79 ("fallen
  off the bus") crashes; see `~/Workspace/System/CLAUDE.md`. On Oath it
  gets `pkg:nvidia` 595.104.02, a recipe in the `desk` profile, and its
  overlay carries the card's settings: GSP off as
  `host:local.module_options`, and `svc:gpu-power` for the caps (off
  until `pkg:nvidia` is present). The kernel has no other display driver
  (no simpledrm, efifb or nouveau), so without `pkg:nvidia` the screen
  stays dark. The desk draws on the card through libglvnd, NVIDIA's GBM
  backend and its Vulkan driver (R41); the monitor is a Dell U4025QW on
  DP-2, and its speakers (the card's DP audio) are NixOS's default sink.
- **Board (R41):** fans on the NCT6798D (`nct6775`, loaded by
  `svc:fan-curve`), temperatures from ASUS's EC (`asus_ec_sensors`, by
  DMI alias). NixOS's fancontrol ramps fans 1–5 and 7 from PWM 100 at a
  T_Sensor (the loop) of 32 °C to 255 at 42 °C; `svc:fan-curve` does the
  same every 5 s and gives the fans back to the board when stopped.
- **Bluetooth:** the AX210's (`btintel`, `ibt-0041-0041`). Paired on
  NixOS: an Xbox Wireless Controller (`68:6C:E6:31:87:67`) and Sony
  WH-CH520 headphones (`14:06:A7:0E:A5:DA`); Oath's BlueZ has its own
  pairings, so each pairs again from the stick.
- **Overlay:** [novus.json](novus.json): hostname, Mountain time, the
  GPU settings, the fan curve. No mounts yet: the trial leaves NixOS's
  disks alone.
- **Oath on it:** the trial stick, Joshua's Samsung Type-C 128 GB
  (`090c:1000`, serial `0354422110003481`; a Bazzite installer before),
  rewritten 2026-10-29 on buccina by `cargo make install --local` from
  desk output `67d5d5c1160c` (R41; its packs published, its release not): ESP
  `OATHESP`, btrfs `oath`, root `UUID=e9cd5765-f83c-4d4d-a451-be9029ccdaa3`.
  In QEMU it built `plan:nvidia` (`pkg:nvidia` `48b78914`, as buccina's
  build; `nvidia.ko` loads and finds no GPU; `pkg:linux-build`
  `b80fd3da` copied in from buccina's store before the bucket had it),
  and `pkg:nvidia` and `svc:gpu-power` are on (generation 1); libglvnd
  loads NVIDIA's EGL vendor and Mesa's, and the Vulkan loader finds
  `nvidia_icd.json`. Its `ssh:local` has buccina's,
  canto's and `root@hub`'s keys; while novus runs the stick, buccina or
  canto drive it:
  `ssh -o UserKnownHostsFile=~/.ssh/novus-oath.known_hosts home@novus` on
  canto, `build/novus-oath.known_hosts` on buccina (the stick's host key,
  not NixOS's). Boot it from the
  firmware's boot menu (F8); NixOS stays the default. It enumerates only
  on a back-panel port: behind the keyboard's USB 2.0 hub it resets four
  times and never becomes a disk.
