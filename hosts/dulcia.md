# dulcia

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Dell OptiPlex 5080 (board `0R7PJT`, BIOS 1.37.0,
  2026-08-30). Intel Core i7-10700T (8 cores, 16 threads), 64 GB. WD
  Green SN3000 NVMe, 1 TB (`nvme0n1`, serial `25152M800175`; GPT:
  512 MiB ESP + btrfs `@`, `@home`, `@work`). Intel graphics (`card0`).
  UEFI. Its RTC holds UTC since 2026-11-06 (`sudo hwclock --systohc
  --utc`, which wrote `/etc/adjtime`); it had run five hours behind, so
  boots started five hours off until chrony stepped them.
- **Network:** DHCP, 10.0.0.146, which local DNS names `dulcia` (MAC
  `70:b5:e8:5b:7d:43`, `net0`, 1 Gb/s). Reach: `ssh dulcia` from buccina
  (`~/.ssh/config`), or `ssh home@10.0.0.146`; host key (`SHA256:pfTb2d1X…6EnEgw`) pinned in
  `build/known_hosts` and buccina's `~/.ssh/known_hosts`, taken on first
  contact. `ssh:local` lists buccina's keys and `home@novus`
  (generation 3, Joshua 2026-11-06); novus reaches it as `ssh dulcia`
  (its local DNS names it, host key in novus's `~/.ssh/known_hosts`).
- **Overlay:** [dulcia.json](dulcia.json) — hostname and Mountain time.
- **Role:** a server, on the headless `base` profile. Installed
  2026-11-06 by `oath install` from the Samsung stick
  ([novus.md](novus.md)), then renamed from `oath` (generation 1).
  Agents change it at Joshua's word: AGENTS.md does not list it with
  canto and buccina. Wicket runs here as `work` (read 2026-11-09):
  `svc:wicket` and its siblings, and 10 workloads `svc:wl-*`, each a
  cell started by `/bin/wicket`, from images in a registry cell on
  127.0.0.1:5000. `svc:wl-b6703b0fa806-web`, the retired `oath`
  workload (R79), is stopped until `wicketctl services install` removes it.
- **Running now:** `base` release `ee3e239caa32` (master `7597cc1`,
  R72 and R73), generation 6, in sync: 5 from `sudo oath update --confirm
  --reboot` on 2026-11-07 (only `pkg:oath` moved), 6 put novus's key in
  `host:local.publishers` beside buccina's (Joshua 2026-11-07); since
  generation 64 it is `names="*" … oath@novus`, any package's head
  (Joshua, 2026-11-10). Mountain time
  (`MST7MDT,M3.2.0,M11.1.0`), over SSH too. `@boot-2` holds the boot
  before it (`57192b33e6ab`).
