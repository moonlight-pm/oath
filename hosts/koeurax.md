# koeurax

Overwrite this file when facts change; do not append a diary.

- **Hardware:** ASUS ROG Strix B760-I, Intel Core i5-13400 (its own
  graphics off in setup), 32 GB. GeForce RTX 4060 (`10de:2882`), on
  NVIDIA 595.104.02 (`pkg:nvidia` 44c0c5e1, built on the stick), on its GSP
  firmware since 2026-11-08. WD_BLACK
  SN7100 1 TB (`nvme0n1`, serial `24493B803192`; 512 MiB ESP + btrfs).
  A 3440-wide display. UEFI. No driver for its Intel Wi-Fi (`8086:7a70`).
  Sleeps in `deep` (S3; `/sys/power/mem_sleep` offers `s2idle [deep]`). Its
  hardware clock kept local time, Windows' habit, so each boot started 7 h
  behind until chrony stepped it; set to UTC on 2026-11-12 (`hwclock
  --systohc --utc --noadjfile`, no `/etc/adjtime`).
  Its Bluetooth is the AX211's (USB `8087:0033`, firmware
  `ibt-1040-0041`): controller up since 2026-11-08. An Xbox Series pad
  (`045E:0B13`, `A8:8C:3E:64:E6:B5`) is paired and trusted over Bluetooth
  LE that day; Steam sees it, and no game has been seen using it.
  Kora's Logitech POP Icon Keys and POP Mouse are bonded and trusted over
  LE, her 2.4 GHz receiver unplugged: first by Sola (`…3C:9C`, `…BC:0F`),
  then re-paired by Joshua on 2026-11-24 (`D9:1C:B0:DF:3C:9D`,
  `D9:A5:29:17:BC:11`; a Logitech device takes a new address at each
  pairing), after they stopped reconnecting after a sleep. The cause was
  the controller resolving addresses: their directed advertising never
  reached the host. Since generation 15 (2026-11-24) `svc:bluetooth` runs
  `bluetoothd -f /run/bluetooth-main.conf`, which its exec writes with
  `[LE] CentralAddressResolution = 0`, and both reconnect after a sleep
  (tested that day: a 60 s sleep, a key and the mouse); when a release
  carries `/etc/bluetooth/main.conf` with the same setting, its exec goes
  back to the seed's. They do not wake
  it from S3 (2026-11-24, three sleeps with the adapter's USB device `1-14`
  and its root hub armed by hand): the kernel set the controller to scan
  passively for its accept list (`Controller Suspended`, state 2), yet no
  advertising report or connection came during or after the sleep, no wake
  count moved on `1-14` or the xHCI, and the resume said "non-Bluetooth wake
  source". The AX211's Bluetooth does not listen or cannot wake the board
  in S3 here; the power button, a USB keyboard or mouse, and `oath wake`
  do.
- **Network:** DHCP, 10.0.0.156 (MAC `60:cf:84:bc:ff:3e`, `net0`, wired
  `igc`). Wake-on-LAN on (`net:net0.wake`, R84, generation 7): on
  2026-11-12 `oath wake 60:cf:84:bc:ff:3e` from buccina woke it from sleep
  in about 4 s (01:35:50Z sent, 01:35:54Z awake); from off untried. Reach: `ssh koeurax` from buccina (the router's DNS names it);
  `ssh:local` lists buccina's two keys. Its host key is not pinned in
  `build/known_hosts`.
- **Overlay:** [koeurax.json](koeurax.json) — hostname.
- **Role:** a desk, `desk` profile, moved from Windows on 2026-11-07 by
  `oath install` from the Samsung stick ([novus.md](novus.md)). Agents
  change it at Joshua's word: AGENTS.md does not list it with canto and
  buccina. Its Windows user's files are on novus, `~/Backups/koeurax`
  (the Modrinth App's profiles under `users/kora/AppData/Roaming/ModrinthApp`).
- **Running now** (read 2026-11-22): the `desk` release `cd27b297c381`, by
  `sudo oath update --confirm --reboot` at Joshua's word on 2026-11-22
  (generation 14; codex and vim moved). It put `/etc/egl` in place, which
  its update to `319b03c43be0` had left out (that update ran an `oath` from
  before R89), so Xwayland had run on Mesa without NVIDIA's GBM and X EGL
  platforms; after the reboot Xwayland loads `libnvidia-egl-gbm`, `-xcb`
  and `-xlib`. Before it, `319b03c43be0`; before that, the `desk` release
  `d4a898a5f53f` (master `e72eaea`, published 2026-11-12 01:32Z), updated
  and rebooted into it that day at Joshua's word: generation 6 (NVIDIA loads with
  `NVreg_PreserveVideoMemoryAllocations=1`), 7 `net:net0.wake`. Before it,
  `f997e9baf436` (2026-11-08): 1 built `plan:steam`, 2 set `host:local`, 3
  that update, 4 and 5 Minecraft's packages (below). A sleep at 01:35:28Z
  through `host:local.power=sleep` armed its 2.4G receiver, `net0` and the
  power button, and the desk kept running across it. A key wakes it and
  the desk redraws (Joshua, 2026-11-12).
  `host:local.publishers` holds buccina's key and, since generation 8
  (2026-11-12, Joshua's hand), `oath@novus` for any package (`names="*"`),
  as on the other four machines, so Sola's own heads reach it at its next
  `oath update` (now `pkg:sola` `90ca782c…`, the release's). Agents may not
  grant a key there. No time zone (UTC).
  Steam runs: Joshua played Terraria and Noita on 2026-11-08, on the GSP
  firmware, with no GPU errors.
- **Minecraft (R74), generations 4 and 5, 2026-11-08:** from master
  (`certs`, `libxi`, `libxtst`, `libxcursor`, `jre-21`, `libarchive`,
  `libqrencode` from `24ca3bb`; `cmark`, `qtbase`, `qtsvg`, `qtimageformats`,
  `qtnetworkauth`, `prismlauncher` `abdc5d15…` from `57dc661`; `0b992383…`
  since generation 6). Each is its own head
  on the origin since that day, signed `oath@buccina`, so `oath update` keeps
  them. Sola's launcher lists it as Minecraft (`Application:` in `state.yaml`,
  `/bin/prismlauncher`). `~/.local/share/PrismLauncher` holds
  `prismlauncher.cfg` and the instance `fabulously-optimized` (1.21.5, Fabric
  Loader 0.16.14): the Modrinth profile "Fabulously Optimized (1)" from the
  Windows backup, its world "Creative" checked file for file against it. Joshua signed in and ran it on 2026-11-08 (OpenGL on
  the RTX 4060, sound on the onboard HDA).
