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
- **Role:** where Oath work happens (R39, Joshua 2026-10-28; builds
  since 2026-10-26): agent sessions, the checkout, self-host, checks
  and publishing, so canto stays free for Sola R&D and novus is
  Joshua's desk. Claude Code 2.1.292 was built here on 2026-10-28
  (`oath build plan:claude-code`, generation 100: novus's pack); not
  signed in by Joshua on 2026-10-28. Reach: `ssh home@10.0.0.149`
  (DHCP; Joshua adds it to the local DNS).
- **SSH to and from canto** (Joshua, 2026-10-28): `ssh canto` uses
  `home@buccina` (`/home/.ssh/id_ed25519`, which canto's `ssh:local`
  lists; `~/.ssh/config` names 10.0.0.3, and canto's host key is pinned
  in `~/.ssh/known_hosts`). `ssh:local` here lists `home@canto`
  (generation 101) beside Joshua's `root@hub` and the Wicket deploy key.
  Agents install, update, reboot and wipe it without asking (Joshua,
  2026-10-26).
- **Running now:** output `fdecb1ec3d68` (R41's libglvnd, BlueZ and
  kernel, R42's `svc:login` programs), pulled on 2026-10-30 after
  `pkg:oath` was moved first by hand (generation 104; its own `oath`
  predated R42's planner) with `oath update --confirm` (generation 105),
  not rebooted: PID 1, the kernel and initrd are still `e433824d38d0`'s,
  and the next boot runs `fdecb1ec3d68`'s, as its QEMU rehearsal from
  `c4cd226761d3` did. The update made `pkg:libglvnd`, `pkg:bluez`,
  `pkg:sbc` and `pkg:readline` and no service: `svc:login`,
  `svc:dbus-system` and `svc:bluetooth` are not here, `svc:screen` has
  no `seat`, so it acts as before (`sola-shell` failed before and
  after: Mesa has no driver for its Intel GPU). Before that,
  `c4cd226761d3` (generation 102), its Sola pins set by hand
  (generation 103). Before that, `e433824d38d0`, pulled with `oath
  update --confirm --reboot` on 2026-10-28 after `pkg:oath`,
  `host:local.profile` and `publishers` were set by hand (generation
  96–97). `svc:pipewire` and `svc:pipewire-pulse` got `rtprio`
  95 by hand (generation 98): the data thread is `SCHED_FIFO`. Before
  that, `f443df24beec` by the push update of 2026-10-27. Since R36 its
  sshd is OpenSSH (generation 3 moved it from dropbear, keeping the
  host key); `pkg:dropbear` was removed (generation 35); `pkg:qemu` and
  `pkg:libslirp` were added for the build-host run (generations 33 and
  36). It passes every metal case that applies to it (44 on 2026-10-27;
  the display and AMD-GPU cases skip: it is headless, and Mesa has no
  Intel driver), sound included: PipeWire takes its HDA card, which
  registers after i915, at boot and on hot-plug. Installed on 2026-10-26 as `55c20bf6a86c` by `cargo make
  install --target root@10.0.0.149 --disk /dev/nvme0n1` over the Wicket
  trial image; that install's reboot landed on an old USB stick, the
  boot order was fixed, and a reboot came up from the NVMe. Root by UUID
  `daaa7fe6-6358-41d5-b968-5760983c1ba7`. Its clock is about 14 s fast
  at boot until chrony steps it.
- **Self-host:** `/home/Projects/Oath`, the working checkout (R39):
  `origin` is `git@github.com:moonlight-pm/oath.git` (novus pushed to
  it until 2026-10-28). Its GitHub key is `/home/.ssh/github_ed25519`
  (`home@buccina (github)`, on Joshua's account as "buccina
  (home@buccina)", 2026-10-28; `~/.ssh/config` names it for
  github.com); git commits as Joshua Kifer. The trial stick's host key
  is in `build/novus-oath.known_hosts`. Scripts and logs:
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
  "oath-store publish (novus)" (Joshua, 2026-10-28). Published on
  2026-10-28: `3b2d49886ff7` (20:51Z, the first), `1fa3b09a85ca`,
  `0c6eebe617d2` and `e433824d38d0` (22:33Z); on 2026-10-29
  `36de66963ac1` and `c4cd226761d3` (18:14Z, the first whose release
  lists Sola's pins); on 2026-10-30 `fdecb1ec3d68` (01:21Z; R41 and
  R42); each after the QEMU cases (61, then 62) and a `cargo make
  update --qemu` from the one machines ran (`--oath-first` for
  `fdecb1ec3d68`, from `c4cd226761d3` and `e433824d38d0`).
