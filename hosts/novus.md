# novus

Overwrite this file when facts change; do not append a diary.

- **Role:** Joshua's daily machine. It runs Oath since 2026-10-30,
  installed by `oath install` from the stick (R43); it was on NixOS 25.11
  before, and Oath's build host until 2026-10-28, when Oath work moved
  to buccina (R39). Nothing changes it or writes its disks without
  Joshua's word.
- **Hardware:** ASUS ProArt X570-Creator WiFi, Ryzen 9 5950X (16 cores,
  32 threads), 128 GB. NVIDIA RTX 3090 Ti (GA102, `10de:2203`) on a
  custom water loop. Realtek ALC1220 audio and the card's HDMI audio.
  Intel AX210 Wi-Fi and Bluetooth, Thunderbolt 4 (Maple Ridge), Nuvoton
  NCT6798D fan and sensor chip, a USB LifeCam Cinema.
- **Network:** the cable is in the Intel I225-V (`igc`,
  `04:42:1a:e7:35:6f`, at 1 Gb/s), DHCP 10.0.0.5. The Aquantia AQC113
  10 GbE (`atlantic`, `04:42:1a:e7:35:6e`) and the Wi-Fi have no link.
- **Disks:** three SATA SSDs, named here by serial: node names change
  between boots (with the stick in, and from NixOS). WD Blue 500 GB
  `21385X801116`: Oath, a 512 MiB ESP `OATHESP` and btrfs `oath` (`@`,
  `@home`), root `UUID=9dbab7bb-c5a7-4996-9b3d-b47a35a0a684`. WD Blue
  2 TB `21351E801362`: NixOS's ESP (2 GiB) and btrfs `home` (`@home`
  with NixOS's `/home/joshua`, `@snapshots`); NixOS's `/etc`, `/root`,
  `/var/lib` (MySQL's data, Redis, samba, Bluetooth) and `/opt/sola`'s
  `share`, `extra` and `nix` are in `/home/joshua/nixos-root` there,
  with `etc-resolved` (the `/nix/store` links followed), copied from the
  stick before the install. SPCC 4 TB `AA250217S304TB00619`: btrfs
  `storage` (`@data`, `@backups`, NixOS's hourly btrbk target).
- **Firmware:** ASUS (AMI). After the install `BootOrder` is `Boot0000`
  `Oath` (the 500 GB's ESP), then `Boot0001` `UEFI OS`, the firmware's
  own entry (AMI's `BO` tag in its optional data) for the fallback
  loader on the 2 TB's NixOS ESP. The install's preview listed `UEFI
  OS` to remove, yet it was there after the reset, and Oath was not
  first: Joshua moved it up in the firmware's setup. Removing the 2 TB's
  ESP (or its `\EFI\BOOT\BOOTX64.EFI`) should end the `UEFI OS` entry.
- **Home (R45):** `/home` is `@oath-home` on the 2 TB's btrfs `home`
  (`UUID=580d861c-4b96-4017-856c-0b55ff4e58ce`, beside NixOS's `@home`
  and `@snapshots`, untouched), set in `host:local.mounts` with
  `subvol=@oath-home,noatime,compress=zstd:1` and mounted by PID 1 at
  boot since 2026-10-30 23:55Z. canto's session made it and copied /home
  in; buccina's re-synced it before the boot. The 500 GB's `@home` and
  the files in `@`'s /home (the mount point) were deleted at Joshua's
  word the same day; the generation and boot snapshots of `@` from before
  (`@gen-1`–`3`, `@boot-1`–`2`) still hold that /home until they rotate
  out. NixOS's home (`@home` on the same 2 TB, with `joshua/`) is
  mounted read-only at `/mnt/nixos-home` for now (generation 4), with
  `X-mount.idmap=b:1000:1:1`: NixOS's uid 1000 reads as `home`, other
  owners as `void`, nothing on disk changes. Next: Joshua copies what he
  picks from it (or the 4 TB's backups) into /home. Generation 5 added
  `DBUS_SYSTEM_BUS_ADDRESS` to `host:local.env`, as the desk profile now has.
- **GPU (R34):** NVIDIA 595.104.02 (`pkg:nvidia` `48b78914`, a recipe in
  the `desk` profile) with GSP off (`NVreg_EnableGpuFirmware=0`, in
  `host:local.module_options`), capped by `svc:gpu-power` at 250 W with
  the core clock locked to 210–1500 MHz, as NixOS ran it after repeated
  Xid 79 ("fallen off the bus") crashes; `svc:gpu-power` runs once per
  boot and exits 0, so it shows `stopped`. The kernel has no other
  display driver (no simpledrm, efifb or nouveau), so without
  `pkg:nvidia` the screen stays dark. The desk draws on the card
  through libglvnd, NVIDIA's GBM backend and its Vulkan driver (R41);
  the monitor is a Dell U4025QW on DP-2, and its speakers (the card's
  DP audio) were NixOS's default sink.
- **Board (R41):** fans on the NCT6798D (`nct6775`, loaded by
  `svc:fan-curve`), temperatures from ASUS's EC (`asus_ec_sensors`, by
  DMI alias). NixOS's fancontrol ramps fans 1–5 and 7 from PWM 100 at a
  T_Sensor (the loop) of 32 °C to 255 at 42 °C; `svc:fan-curve` does the
  same every 5 s and gives the fans back to the board when stopped.
- **Bluetooth:** the AX210's (`btintel`, `ibt-0041-0041`). Paired on
  NixOS: an Xbox Wireless Controller (`68:6C:E6:31:87:67`) and Sony
  WH-CH520 headphones (`14:06:A7:0E:A5:DA`); Oath's BlueZ has its own
  pairings, so each pairs again; NixOS's are in
  `nixos-root/var/lib/bluetooth`.
- **Overlay:** [novus.json](novus.json): hostname, Mountain time, the
  GPU settings, the fan curve, `/home` on the 2 TB's `@oath-home`
  (`/mnt/nixos-home` is temporary, not in it).
- **Oath on it:** installed 2026-10-30 by `oath install 21385X801116
  --confirm` from the stick: desk output `cf238ebe3360` (never published)
  at generation 0, with no `/oath/output.json` (R45 makes installs write
  one). Joshua's `sudo oath update --confirm` then moved it to the older
  published `fed6fb141a5e` (generation 1, 22:39Z), whose PID 1 has no
  `@home`. At 23:54Z buccina updated it to `86dd0d999ce4` (generation 2:
  `pkg:oath`, `pkg:rivulet`), set /home (generation 3) and rebooted it:
  PID 1 is the output's, in sync, 21 services, the desk on the card. Its `ssh:local`
  has buccina's, canto's and `root@hub`'s keys; buccina reaches it with
  `ssh -o UserKnownHostsFile=build/novus-oath.known_hosts home@10.0.0.5`
  (the host key the install printed, ending `Suecii`). canto's
  `~/.ssh/novus-oath.known_hosts` still has the old stick's key.
- **The stick:** Joshua's Samsung Type-C 128 GB (`090c:1000`, serial
  `0354422110003481`), desk output `cf238ebe3360` with novus's overlay
  and `pkg:nvidia` (generation 1), root
  `UUID=fcffae46-5f62-4304-97f4-d130910e66e5`; its host key is no
  longer pinned. Boot it from the firmware's boot menu (F8), from a
  back-panel port: behind the keyboard's USB 2.0 hub it resets four
  times and never becomes a disk.
