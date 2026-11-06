# dulcia

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Dell OptiPlex 5080 (board `0R7PJT`, BIOS 1.37.0,
  2026-08-30). Intel Core i7-10700T (8 cores, 16 threads), 64 GB. WD
  Green SN3000 NVMe, 1 TB (`nvme0n1`, serial `25152M800175`; GPT:
  512 MiB ESP + btrfs `@`, `@home`, `@work`). Intel graphics (`card0`).
  UEFI. Its RTC runs five hours behind UTC (18:05 at 23:05 UTC, likely
  local time from an earlier OS), so each boot starts five hours off
  until chrony steps it, about 8 s in.
- **Network:** DHCP, 10.0.0.169 (MAC `70:b5:e8:5b:7d:43`, `net0`,
  1 Gb/s). Reach: `ssh dulcia` from buccina (`~/.ssh/config`), or
  `ssh home@10.0.0.169`; host key (`SHA256:pfTb2d1X…6EnEgw`) pinned in
  `build/known_hosts` and buccina's `~/.ssh/known_hosts`, taken on first
  contact. `ssh:local` lists buccina's keys and `home@novus`
  (generation 3, Joshua 2026-11-06); novus reaches it as `ssh dulcia`
  (its local DNS names it, host key in novus's `~/.ssh/known_hosts`).
- **Overlay:** [dulcia.json](dulcia.json) — hostname and Mountain time.
- **Role:** a server, on the headless `base` profile. Installed
  2026-11-06 by `oath install` from the Samsung stick
  ([novus.md](novus.md)), then renamed from `oath` (generation 1).
  Agents change it at Joshua's word: AGENTS.md does not list it with
  canto and buccina.
- **Running now:** `base` release `57192b33e6ab` (master `c76f4be`),
  generation 4, in sync, from `sudo oath update --confirm --reboot` on
  2026-11-06 (only `pkg:oath` moved). Mountain time
  (`MST7MDT,M3.2.0,M11.1.0`), over SSH too. `@boot-1` holds the boot
  before it (`01f12aaf944d`).
