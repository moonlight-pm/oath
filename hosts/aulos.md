# aulos

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Dell OptiPlex 5000 (board `03W3VW`, BIOS 1.41.0), the same
  model as buccina. Intel Core i7-12700T (12 cores, 20 threads), 64 GB. SK
  hynix PC801 NVMe, 1 TB (`nvme0n1`, serial `AMD9N000414509H6N`; GPT: 512 MiB
  ESP + btrfs `oath` with `@` and `@home`, root `UUID=a6552e5d-7218-4a9a-b088-01e104b3694e`).
  Intel graphics (`card0`). UEFI. No monitor or keyboard attached.
- **Network:** DHCP, 10.0.0.169, which local DNS names `aulos` (MAC
  `08:92:04:e2:2c:bc`, `net0`). Reach: `ssh aulos` from novus or buccina, as
  `home`; host key `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIKo41IVgqYCwclwurpekn59a/7fxGpBOtOfl/pdu7lm`,
  pinned in both machines' `~/.ssh/known_hosts` and buccina's
  `build/known_hosts`.
- **Overlay:** [aulos.json](aulos.json): hostname, Mountain time, the
  publishers dulcia trusts (`oath@buccina`, and `oath@novus` for any package),
  and `ssh:local` with `home@buccina`'s two keys and `home@novus`.
- **Firmware:** `Boot0005 Oath` (the NVMe's `\EFI\BOOT\BOOTX64.EFI`) is first,
  then the NIC's IPv4, IPv6 and HTTPs boots. `oath install` removed the
  firmware's entries for the NVMe and the stick. It booted Oath from the NVMe
  with the stick still in.
- **Role:** a server on `base`, and with dulcia the first members of a consort
  (R87, [docs/specs/consort.md](../docs/specs/consort.md)). Agents installed it
  at Joshua's word ("aulos is yours to wipe", 2026-11-12); other changes wait
  for his word, as on dulcia.
- **Installed 2026-11-12:** `cargo make install --local` on buccina wrote the
  Samsung stick with `base` release `c1d413829997` and this overlay. aulos's
  NixOS was the old Wicket host (Postgres, Stalwart's mail, the IdP, from
  before Wicket moved to dulcia). Joshua said wipe it as is, without a copy. buccina's
  root key set `BootNext` to the stick from that NixOS. From the stick,
  `sudo oath install AMD9N000414509H6N --confirm` wrote the NVMe.
- **Running now:** `base` release `c1d413829997`, its record written by
  `sudo oath update --confirm` (2026-11-12, moved nothing), with the consort
  (R87) as an overlay since 2026-11-13: `pkg:oath` `sha256-862f35dd…` from
  `consort@43ad5e5` (built on buccina by `cargo make pkgs oath`) and
  `svc:oath-bus` made by hand (generations 1 and 2), then PID 1 re-executed
  into that pack's `init` (`sudo oath reexec-init`) so `/oath/schema` knows
  `peer`. In consort `fb51071498a5` with canto, which it admitted (code
  compared) on 2026-11-13. `oath update` replaces the overlay until a release
  carries it. 6 services, in sync.
