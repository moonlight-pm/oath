# koeurax

Overwrite this file when facts change; do not append a diary.

- **Hardware:** ASUS ROG Strix B760-I, Intel Core i5-13400 (its own
  graphics off in setup), 32 GB. GeForce RTX 4060 (`10de:2882`), on
  NVIDIA 595.104.02 (`pkg:nvidia` 44c0c5e1, built on the stick). WD_BLACK
  SN7100 1 TB (`nvme0n1`, serial `24493B803192`; 512 MiB ESP + btrfs).
  A 3440-wide display. UEFI. No driver for its Intel Wi-Fi (`8086:7a70`),
  no firmware for its Bluetooth (`ibt-1040-0041`).
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
- **Running now:** the stick's desk output `06e0194eb4f3` (master
  `fc74baa`, never published), newer than the published `6c8d38bd54c4`;
  generation 2: 1 built `plan:steam` (`pkg:steam`), 2 set `host:local`.
  `host:local.publishers` is buccina's key alone. No time zone (UTC).
  Steam runs, and has run a game (SteamWorld Dig 2 left its save folder).
