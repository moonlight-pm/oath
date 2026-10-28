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
- **Running now:** build output `f443df24beec` (the `desk` profile:
  Linux 7.3 and the 2026-10-27 security sweep), by `cargo make update
  --reboot` on 2026-10-27. Since R36 its sshd is OpenSSH (generation 3
  moved it from dropbear, keeping the host key); `pkg:dropbear` was
  removed (generation 35); `pkg:qemu` and `pkg:libslirp` were added for
  the build-host run (generations 33 and 36). It passes every metal case
  that applies to it (44 on 2026-10-27; the display and AMD-GPU cases
  skip: it is headless, and Mesa has no Intel driver), sound included:
  PipeWire takes its HDA card, which registers after i915, at boot and
  on hot-plug. Installed on 2026-10-26 as `55c20bf6a86c` by `cargo make
  install --target root@10.0.0.149 --disk /dev/nvme0n1` over the Wicket
  trial image; that install's reboot landed on an old USB stick, the
  boot order was fixed, and a reboot came up from the NVMe. Root by UUID
  `daaa7fe6-6358-41d5-b968-5760983c1ba7`. Its clock is about 14 s fast
  at boot until chrony steps it.
- **Self-host:** `/home/Projects/Oath` (pushed from novus),
  `build/selfhost.sh` in tmux session `selfhost`; log
  `build/selfhost.log`, result `build/selfhost-result.txt`. Run 1
  (2026-10-26, `c5615fb`): bootstrap 32 min, `pkgs` 1 h 52 min, 158 of
  163 products as committed. Run 2 (`b34d189`, 2026-10-26 23:43Z to
  02:07Z): the same times, 163 of 163 as committed. Security sweep
  (2026-10-27, `build/buildhost.sh` in tmux `buildhost`, log
  `build/buildhost.log`): `pkgs -j 4` from the committed lock gave the
  162 products novus got; the 135 after the base took 26 min.
- **Publisher (R38):** buccina publishes the outputs machines pull.
  Its signing key is `/home/.config/oath/sign_ed25519` (made
  2026-10-28; public half `oath@buccina`,
  `SHA256:PjyzPel8Vz4kHkbMUpFUEWigcvdJs4uOs71fMxBzF9w`, in the `base`
  profile's `host:local.publishers`). The bucket's write key is
  `/home/.config/oath/origin.env` (mode 600), a copy of novus's token
  "oath-store publish (novus)" (Joshua, 2026-10-28). First publish
  2026-10-28T20:51Z: release `3b2d49886ff7` for `desk` and `pkg:oath`
  `sha256-6e563f2d6c69` (log `build/publish.log`).
