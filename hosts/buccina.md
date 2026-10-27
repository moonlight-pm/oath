# buccina

Overwrite this file when facts change; do not append a diary.

- **Hardware:** Dell OptiPlex 5000 Micro. Intel Core i7-12700T (12
  cores, 20 threads), 64 GB. SK hynix PC801 NVMe, 1 TB (`nvme0n1`; GPT:
  512 MiB ESP + btrfs `@`). Intel e1000e Ethernet. No monitor or
  keyboard attached.
- **Boot order:** the reinstall changed the NVMe's partition IDs, and
  the firmware then put an old `--usb` installer stick (`Boot0004`)
  first; on 2026-10-26 `BootOrder` was set to `0000 0004 0001 0002 0003`
  from Oath through efivarfs, and Joshua pulled the stick. `Boot0000` is
  the NVMe's `\EFI\Boot\BootX64.efi`, which is `oath-efi`.
- **Network:** DHCP, 10.0.0.149 (MAC `08:92:04:e3:29:15`, `net0`).
  Reach: `ssh home@10.0.0.149`, host key pinned in `build/known_hosts`.
- **Overlay:** [buccina.json](buccina.json) — hostname and Mountain time.
- **Role:** the Oath build machine (Joshua, 2026-10-26): self-host and
  other long Oath work run here, so canto stays free for Sola R&D.
  Agents install, update, reboot and wipe it without asking (Joshua,
  2026-10-26).
- **Running now:** build output `a14d1acd3c2a` (the `desk` profile, R36:
  OpenSSH's sshd, `void`), by `cargo make update --reboot` on
  2026-10-27 (generation 3 moved `svc:sshd` from dropbear; its host key
  is the one it had, converted). Installed on 2026-10-26 as
  `55c20bf6a86c` by `cargo make install --target root@10.0.0.149 --disk
  /dev/nvme0n1` over the Wicket trial image (reached with the Wicket
  deploy key); that install's reboot landed on the USB stick, the
  installed kernel was started by kexec from it, the boot order fixed,
  and a reboot came up from the NVMe. Root by UUID
  `daaa7fe6-6358-41d5-b968-5760983c1ba7`. `pkg:dropbear` is still
  present, unused. Its clock is about 14 s fast at boot until chrony
  steps it.
- **Self-host:** `/home/Projects/Oath` (pushed from novus),
  `build/selfhost.sh` in tmux session `selfhost`; log
  `build/selfhost.log`, result `build/selfhost-result.txt`. Run 1
  (2026-10-26, `c5615fb`): bootstrap 32 min, `pkgs` 1 h 52 min, 158 of
  163 products as committed. Run 2 (`b34d189`, 2026-10-26 23:43Z to
  02:07Z): the same times, 163 of 163 as committed.
