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
  signed in by Joshua on 2026-10-28. Codex 0.164.0 was built here on 2026-11-02
  (`oath build plan:codex`, generation 158), as it ships. Go run by hand uses
  `GOPATH=/home/.local/share/go` (`~/.config/go/env`), not `~/go`. Reach: `ssh home@10.0.0.149`
  (DHCP; Joshua adds it to the local DNS).
- **SSH to and from canto** (Joshua, 2026-10-28): `ssh canto` uses
  `home@buccina` (`/home/.ssh/id_ed25519`, which canto's `ssh:local`
  lists; `~/.ssh/config` names 10.0.0.3, and canto's host key is pinned
  in `~/.ssh/known_hosts`). `ssh:local` here lists `home@canto`
  (generation 101) and `home@novus` (generation 108, 2026-10-31, Joshua)
  beside Joshua's `root@hub` and the Wicket deploy key.
  Agents install, update, reboot and wipe it without asking (Joshua,
  2026-10-26).
- **Running now:** output `e73aa522ec58` (master `1678f60`, generation 166), from the
  audit's `oath update --confirm --reboot` on 2026-11-04: Linux 7.3.1 with its 12 io_uring
  patches, TIOCSTI off (R60), Landlock and Yama (R64), Codex 0.165.0, in sync, its overlays
  kept. Before it, `ca0f80bd441a` (master `75def41`, generation 162, 2026-11-03).
  Before it, `1c09695ec24e` (master `6d20da0`, generation 133, 2026-11-02), at Joshua's
  word and not published: `cargo make publish --dest` into a directory, served on loopback
  as a second origin for `oath update --output`, after a rehearsal from `1caabcfb0b8c`.
  `host:local.origins` is the store alone again. Before it,
  `1caabcfb0b8c` (Sola master@c1bde1c4 on CEF
  156.0.15, `pkg:sola-vault` and `svc:sola-vault`), pulled 2026-11-01
  with `oath update --confirm --reboot` (generation 115), `3c4a23053b7a` (the
  first audit's CEF 156.0.15 and expat release, generation 112),
  `178ac39ce817` (Linux 7.3.1, generation 111) and Release B
  (`dd0f847aa9c9`, generation 110).
  `svc:audit` (R48, this file's overlay: `check/audit/run` as `home`,
  schedule `0 4 * * *`) since generations 113-114. `pkg:qemu` 11.1.3
  (`0b1ad2cc`, from master@9e60f4f, an overlay no release sets) since
  generation 116; `cargo make check` passed 65/65 on it. Not here, listed by
  every update and now counted as the owner's to add: `svc:login`,
  `svc:dbus-system`, `svc:bluetooth`, `svc:sola-sessions`,
  `pkg:libglvnd-i386`. `pkg:llama-cpp` b11960 (`0908155920628b60`, from master@d3a4d4b) and `pkg:writ` 0.1.0 (`1a32e024c217`, from
  master@fd67f8a, the first with OpenRouter, no key yet), overlays no release sets, since generation 171 (2026-11-05). No `svc:llama`: `writ`
  loads a test model, Qwen3 1.7B Q4, from `~/.local/share/writ/models`, as `work` (R68). It is headless and runs no desk (Joshua, 2026-11-03; generation 161,
  and in its overlay): `svc:rivulet`, `svc:seatd`, `svc:sola-river`, `svc:sola-shell`,
  `svc:sola-session`, `svc:sola-bus`, `svc:sola-call`, `svc:sola-vault`, `svc:pipewire`,
  `svc:pipewire-pulse`, `svc:wireplumber`, `svc:dbus-session`, `svc:screen` and `svc:sola-kvm`
  are off. An update leaves them off, and still brings Sola's packs, since it follows `desk`.
  `svc:sola-tmux` and `svc:sola-tmux-ws` stay: Projects on canto and novus reach this machine
  over ssh, in `tmux -L sola-ws`. `dmesg`'s one error is its Intel GPU's: i915 asks for
  `i915/tgl_guc_70.bin`, and `/lib/firmware/i915` is empty (seen 2026-11-02). Since R36 its
  sshd is OpenSSH (generation 3 moved it from dropbear, keeping the
  host key); `pkg:dropbear` and `pkg:lzo` are gone (generation 163); `pkg:qemu` and
  `pkg:libslirp` were added for the build-host run (generations 33 and
  36). It passed every metal case that applied to it (44 on 2026-10-27;
  the display and AMD-GPU cases skip: it is headless, and Mesa has no
  Intel driver), sound included while PipeWire ran. With audio off since 2026-11-03, expect
  the metal case `desk-sound` to fail here: nothing skips it. Installed on 2026-10-26 as `55c20bf6a86c` by `cargo make
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
  R42) and `fed6fb141a5e` (13:58Z; the update builds recipes and
  spares the seat, 63 cases); each after the QEMU cases (61, then 62) and a `cargo make
  update --qemu` from the one machines ran (`--oath-first` for
  `fdecb1ec3d68`, from `c4cd226761d3` and `e433824d38d0`).
