# novus

Overwrite this file when facts change; do not append a diary.

- **Role:** Joshua's daily machine and Oath's build host, on NixOS 25.11
  (`/etc/nixos/configuration.nix`). It leaves NixOS for its own Oath
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
  until `pkg:nvidia` is present).
- **Overlay:** [novus.json](novus.json): hostname, Mountain time, the
  GPU settings. No mounts yet: the trial leaves NixOS's disks alone.
- **Oath on it:** the trial stick, Joshua's Samsung Type-C 128 GB
  (`090c:1000`, serial `0354422110003481`; a Bazzite installer before),
  written 2026-10-25 by `cargo make install --local` from `55c20bf6a86c`:
  ESP `OATHESP`, btrfs `oath`, root `UUID=4b160377-a07a-43f7-bffe-0ee0738f26bf`,
  booted once in QEMU. `ssh:local` has novus's key and canto's `home`
  key: while novus runs the stick it is driven from canto,
  `ssh -o UserKnownHostsFile=build/novus-oath.known_hosts home@novus` in
  canto's checkout (the stick's host key, not NixOS's). Boot it from the
  firmware's boot menu (F8); NixOS stays the default. It enumerates only
  on a back-panel port: behind the keyboard's USB 2.0 hub it resets four
  times and never becomes a disk.
