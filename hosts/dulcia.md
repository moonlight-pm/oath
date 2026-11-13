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
- **Overlay:** [dulcia.json](dulcia.json) — hostname, Mountain time and
  the NAS at `/mnt/alpha`.
- **NAS (R90):** `host:local.mounts` has `/mnt/alpha` from
  `nas:/mnt/alpha` (10.0.0.12), no options: NFS 4.1, read-write, `hard`
  (generation 84, Joshua 2026-11-12). The export maps every client user
  to root, so every user here, `work` and Wicket's cells too, has root's
  rights on it: Joshua chose `/mnt` over `/home/alpha`, which would have
  kept `work` and `void` out.
- **Role:** a server, on the headless `base` profile. Installed
  2026-11-06 by `oath install` from the Samsung stick
  ([novus.md](novus.md)), then renamed from `oath` (generation 1).
  Agents change it at Joshua's word: AGENTS.md does not list it with
  canto and buccina. Wicket runs here as `work` (read 2026-11-09):
  `svc:wicket` and its siblings, and 10 workloads `svc:wl-*`, each a
  cell started by `/bin/wicket`, from images in a registry cell on
  127.0.0.1:5000. `svc:wl-b6703b0fa806-web`, the retired `oath`
  workload (R79), is stopped until `wicketctl services install` removes it.
- **Running now** (read 2026-11-13): `base` release `985cfbd7490f`
  (2026-11-13), generation 84, in sync. Generation 83 took it by `sudo
  oath update --confirm --keep pkg:postgresql --keep pkg:postgis --keep
  pkg:cloudflared`, no reboot: `pkg:oath`, `expat` and `git`'s needs
  moved, and PID 1 re-executed into the output's with every service
  running; the kernel and initrd on the ESP are the output's from the
  next boot. The three kept packages wait at their old heads, since
  taking them restarts Wicket's PostgreSQL (to 18.7, 52 CVEs) and
  cloudflared (VULN-170057, an Access bypass): `sudo oath update
  --confirm` takes them. Generation 84 mounted the NAS (above).
  Generations 6 and 64 put novus's key in `host:local.publishers`
  beside buccina's, as `names="*" … oath@novus` (Joshua, 2026-11-07 and
  2026-11-10); most generations since are Wicket's services and
  workloads. Mountain time (`MST7MDT,M3.2.0,M11.1.0`), over SSH too.
