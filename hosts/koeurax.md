# koeurax

Overwrite this file when facts change; do not append a diary.

- **Hardware:** ASUS ROG Strix B760-I, Intel Core i5-13400 (its own
  graphics off in setup), 32 GB. GeForce RTX 4060 (`10de:2882`), on
  NVIDIA 595.104.02 (`pkg:nvidia` 44c0c5e1, built on the stick), on its GSP
  firmware since 2026-11-08. WD_BLACK
  SN7100 1 TB (`nvme0n1`, serial `24493B803192`; 512 MiB ESP + btrfs).
  A 3440-wide display. UEFI. No driver for its Intel Wi-Fi (`8086:7a70`).
  Its Bluetooth is the AX211's (USB `8087:0033`, firmware
  `ibt-1040-0041`): controller up since 2026-11-08. An Xbox Series pad
  (`045E:0B13`, `A8:8C:3E:64:E6:B5`) is paired and trusted over Bluetooth
  LE that day; Steam sees it, and no game has been seen using it.
- **Network:** DHCP, 10.0.0.156 (MAC `60:cf:84:bc:ff:3e`, `net0`, wired
  `igc`). Reach: `ssh koeurax` from buccina (the router's DNS names it);
  `ssh:local` lists buccina's two keys. Its host key is not pinned in
  `build/known_hosts`.
- **Overlay:** [koeurax.json](koeurax.json) — hostname.
- **Role:** a desk, `desk` profile, moved from Windows on 2026-11-07 by
  `oath install` from the Samsung stick ([novus.md](novus.md)). Agents
  change it at Joshua's word: AGENTS.md does not list it with canto and
  buccina. Its Windows user's files are on novus, `~/Backups/koeurax`
  (the Modrinth App's profiles under `users/kora/AppData/Roaming/ModrinthApp`).
- **Running now:** the `desk` release `f997e9baf436` (master `89cc9eb`,
  published 2026-11-08), rebooted into it that day; generation 3: 1 built
  `plan:steam` (`pkg:steam`), 2 set `host:local`, 3 the update.
  `host:local.publishers` is buccina's key alone, so it refuses Sola's own
  heads (novus's key) and takes Sola with releases. No time zone (UTC).
  Steam runs: Joshua played Terraria and Noita on 2026-11-08, on the GSP
  firmware, with no GPU errors.
- **Minecraft (R74), generations 4 and 5, 2026-11-08:** from master
  (`certs`, `libxi`, `libxtst`, `libxcursor`, `jre-21`, `libarchive`,
  `libqrencode` from `24ca3bb`; `cmark`, `qtbase`, `qtsvg`, `qtimageformats`,
  `qtnetworkauth`, `prismlauncher` `abdc5d15…` from `57dc661`). Each is its own head
  on the origin since that day, signed `oath@buccina`, so `oath update` keeps
  them. Sola's launcher lists it as Minecraft (`Application:` in `state.yaml`,
  `/bin/prismlauncher`). `~/.local/share/PrismLauncher` holds
  `prismlauncher.cfg` and the instance `fabulously-optimized` (1.21.5, Fabric
  Loader 0.16.14): the Modrinth profile "Fabulously Optimized (1)" from the
  Windows backup, its world "Creative" checked file for file against it. Joshua signed in and ran it on 2026-11-08 (OpenGL on
  the RTX 4060, sound on the onboard HDA).
