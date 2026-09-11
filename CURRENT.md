# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-09-11

---

## Now

1. **T39 Omarchy session payload (keep Sola).** `host:local.session`
   = `sola` | `omarchy` (seed default **sola**; `--confirm` to switch).
   Exclusive compositor. **Canto is on `omarchy`:** Hyprland 0.52.2 as
   `home` on amdgpu DP-10, Philips 1920×1080@60, `wayland-1`, River/Sola
   stopped. `svc:omarchy-shell` Quickshell 0.2.1 paints `omarchy-bar`
   1920×26 (uid 1; gen 51). `pkg:quickshell` + `pkg:omarchy` packed;
   `$OMARCHY_PATH=/oath/store/pkg/omarchy`. PID 1 waits 3 s for DRM on
   compositor stop. ESP last-5 is oath + boots 12–8; initrd `/init` is
   T39 (1028640). **Rebooted onto that initrd:** kernel **7.3.0-rc1 #4**,
   Hyprland pid 362 and omarchy-shell pid 367 as `home` (restarts=0),
   `omarchy-bar` 1920×26, no River, no crash-loop. QEMU image packed
   (8G rootfs). Probe: T39 pack facts ok (`pkg:hyprland` /
   `quickshell` / `omarchy` present, session sola, hyprland/omarchy-shell
   off). Later probe steps fail from dbus/pipewire serial spam.
   Freeze:
   [docs/specs/2026-09-10-omarchy-session.md](docs/specs/2026-09-10-omarchy-session.md).
2. **T31 on canto: `ssh home@canto`.** uid 1, `HOME=/home`, sudo ALL
   no password, groups `root`+`home` only. Graphical stack **on**
   as `home` (River GLES2/radeonsi + Sola on amdgpu DP-10, Philips
   1920×1080). `/dev/ptmx` is 0666 so terminal tmux/PTY works.
   Serial svc **off** (no UART). EFI mark still paints. **T30**
   `pkg:grok` packed (`/bin/grok`, updater off). Guest `/bin/xdg-open`
   is `solactl open` so Grok login can raise sola-browser. `pkg:git` and
   `pkg:curl` packed. Menubar CPU/MEM/RX LED graphs paint (image
   raster, not 1×1 canvas). Volume chip: PipeWire trio is catalog
   `svc:*` (canto gen 40) as `home`; default sink **Built-in Audio**.
   Live PID 1 predates `pipewire` in `is_seat_svc`, so exec is
   `/bin/env sola-audio=1 /bin/pipewire` (the `sola-` token makes
   spawn drop uid 1). sola-shell already has a `pw-cat` meter on
   that sink. Bluetooth chip: `pkg:bluez` (system dbus + bluetoothd)
   gen 41; Apple BCM20702 `hci0` (`05ac:828d`) powered
   (`bluetoothctl show`). zbus talks
   `/var/run/dbus/system_bus_socket` (symlink to `/run/dbus/…`);
   agent registered. Glyph is a 14px lucide rune **left of the
   volume spectrum**. Modules are live this boot; ESP initrd now
   packs `btusb`/`bluetooth` (reboot to persist). **scp / sftp** as `home@canto` (dropbear
   `sftp-server` + `scp`; guest `/bin/ssh` is musl OpenSSH; busybox `/bin/vi`). **sola-kvm client** on
   canto (`/bin/sola-kvm listen`, UDP 4242); novus peer is canto
   10.0.0.3 1920×1080 (Mac 10.0.0.133 unconfigured from this desk).
   KVM clipboard: CLIP1 TCP same port, text + `image/png` on Enter/Leave.
   Super+Tab over kvm confirms on Super-up (virtual-keyboard **key
   before modifiers**; kernel auto-repeat not injected so Super/Alt
   cannot stick). `/oath/store/pkg/sola/libexec/sola-kvm` hand-copied
   this boot, oath-sola `386c9d78`.
   **`pkg:thoxa`** is the `home` login shell (`/bin/thoxa`; `/etc/shells`
   lists `/bin/thoxa` and `/bin/sh`; `host:local.env` `SHELL` /
   `THOXA_ROOT`). Store hand-copied this boot from Thoxa `c42c9a6`
   (session-rc-split + D156 emacs line editor; `ssh home@canto echo hi`
   is `thoxa -c`). Seat rc is Oath-local (`std/prompt`; no NixOS `path()`). sola-terminal and workspaces tmux inherit `$SHELL`
   (pkg:sola wrappers default `/bin/thoxa`, not ash). Wrappers `cd
   $HOME` so a new Terminal is not `/` (PID 1 cwd). Root/serial
   stay `/bin/sh`. Catalog env is `/etc/profile` only; live `pkg:oath`
   does not write `$HOME/.profile`. **T33** off-box backup: one NFS copy on nas
   `10.0.0.12:/mnt/alpha/backup/canto` (`canto.send` gen 16, ~1.9G,
   checksum match). `svc:backup` sleeps until **04:00 Mountain** then
   sends (`backup-daily`; nfs modules this boot). **T34** Sola clock
   is `host:local.timezone` (Mountain POSIX); `date` stays UTC. **T35**
   guest toolchain live (gen 19): `pkg:cc` Zig 0.16, `pkg:rustc` 1.98.1,
   `pkg:cmake` 4.3.5 + ninja, `pkg:pkg-config` empty farm. Official
   tarballs, no Nix, no rustup. **T37** Steam runtime on
   canto (gen 21): `/bin/bash` (GNU 5.2.15 static musl),
   `/bin/Xwayland` 24.1.13, `/bin/steam` (bootstrap extracts;
   32-bit ELF loads; ubuntu12 client on disk; `steamui.so` loads
   with 32-bit GL from steamrt3c). **Session Steam** is a normal
   X11 window on River **`+xwayland`** (`DISPLAY=:0`,
   `WLR_XWAYLAND=/bin/Xwayland`; user confirmed the library
   paints). Super+Q / Sola close: `STEAM_FRAME_FORCE_CLOSE=1` on
   `:0` is not enough (hide-to-tray); River `Window.close` SIGTERMs
   X11 class `steam` (live `libexec/river` `699a166`). Launcher
   lists **Steam** (`~/.config/sola/shell/applications.json` +
   `state.yaml` `Application`; `/bin/steam`; `lucide/gamepad-2`).
   **`pkg:mesa`** is GLX plus Vulkan WSI (RADV 26.2.1). 32-bit RADV
   loads (Debian `libwayland-client` 1.26 in `pkg:mesa/lib32` for
   `wl_fixes_interface`; steamrt 0.3.0 is too old; ICD uses DT_RPATH).
   GpuTopology reports RADV PITCAIRN. `liboath-peercred.so` is in
   the steamui **dlmopen** namespace via a patched copy
   (`ubuntu12_32/steamui.oath.so`); do **not** patchelf live
   `steamui.so`. steamwebhelper `_v2-entry-point` passes
   `--disable-gpu` (CEF GPU process SIGBUS 135 on RADV SI). Session
   Xwayland is **`-glamor off`** (river radeonsi SIGBUS’d on SI
   tiled BOs). 64-bit `liboath-glclass.so` redirects `libGL`/`libEGL`
   dlopen to `pkg:mesa` so `ubuntu12_64/gldriverquery` is no longer
   ELFCLASS32. `CLONE_NEWUSER` EPERM from PID 1 chroot. 32-bit
   `libgbm` is in `pkg:mesa/lib32`. steam-runtime `compose.dir`
   aliases C.UTF-8. Novus Steam is still nixpkgs `*-bwrap` FHS,
   not this pack. **Gamescope nest on SI is parked** (tag
   `t37-gamescope-canto`). Canto gen 39: `pkg:gamescope`
   `present=false`, store removed, `/bin/sola-arcade` unlinked.
   `/bin/steam` and `/bin/Xwayland` stay. Apply refuses
   `pkg:gamescope` `present=true` without
   Vulkan WSI DRM modifiers (Pitcairn `1002:6810`); `sola-arcade`
   farm link is skipped on the same GPUs. SI nest patches, pool
   layer, `libdecor-oath` removed from the tree. **tmux:** new
   Terminal tabs and workspaces splits work (do not stub
   `pkg:glibc` `libresolv`). A second sola-terminal process used to
   retract the first's new tabs (Sola-generic fix packed this boot).
   **T36** other kit names still out (`alsa.pc`). Zig `cc` host link
   is `image/oath-cc-link.sh` + `zig-gnu-cc.sh`. Next: a `.pc` for
   alsa, or confirm Super+Q reaps `ubuntu12_32/steam`. Do not
   resurrect the canto gamescope nest.
   `lo` is up this boot (`127.0.0.1`); PID 1 `unix_floor` also
   mkdirs `/run/dbus` and `/var/run/dbus` (next reboot).
   **T38** firmware boots on canto ESP: last five archives + current.
   `oath-efi` menu (metal timeout 5, QEMU EFI 0). PID 1 honors
   `oath.subvol=@` / `@boot-N` / `@gen-N`. Ubuntu mainline 7.3-rc1
   **panicked** on canto (PID 1 died after dhcp). Kernel lock amended:
   vanilla kernel.org **7.3.0-rc1** compiled on the desk
   (`image/build-linux.sh`, nice 10, `-j16`; `joshua@novus`, gcc 14.3).
   ESP **Oath** default is 7.3 DPM + BT modules + unix_floor dbus
   dirs (`subvol=@`). Last-5: **boot 12** (T39 initrd), 11, 10, 9, 8.
   **boot 7** pruned this rotate. systemd-boot stays
   `BOOTX64` so the 5 s list is visible (oath-efi logo still hides
   the picker). Live process is **7.3.0-rc1 #4** (this reboot).
   `force high` is **850/1270 MHz**
   (level 3 of 4). VBIOS `HARDWAREDC` was floating the DC GPIO so the
   SMC clamped to 300/150 and `SetForcedLevels` returned 0xff. `pkg:mesa` live is Debian **26.2.1**
   plus **libdrm 2.4.134** and **32-bit libgbm**. card1 DP-10 is up.
   Small SI DPM patches live under `image/linux-patches` (vanilla
   tarball + fragment + patches; not a linux.git fork). **`cargo make
   boot` on the desk:** `OATH_KERNEL` / `OATH_MODULES` /
   `OATH_FIRMWARE` / `OATH_BUSYBOX`; musl linker is rustup `rust-lld`
   (zig cc duplicates musl crt). NixOS has no `/bin/true` — `tools.rs`
   uses `true` on PATH. **Do not** `cargo make esp --esp /dev/sda1` on
   novus (that is novus’s disk). Pack on novus, rotate the ESP **on
   canto**. Must pack Pitcairn firmware (`build/linux/firmware` or
   `OATH_FIRMWARE`); a DPM initrd without `pitcairn_mc.bin` kills both
   GPUs. ESP initrd includes the blobs + `btusb`. DPM: skip `GPIO_DC`
   when `ac_power`; `force high` **850/1270**.
   `gpu_busy_percent` is ENOTSUP. Session Xwayland is `-glamor off` (no
   SIGBUS). Steam-runtime `compose.dir` already aliases C.UTF-8.
   Spare GPU idea: `docs/ideas/2026-09-07-canto-second-pitcairn.md`.
   Do not `cargo make install --confirm` (wipe).
3. **T27 metal canary is in.** `ssh home@canto`. `host:local` canto,
   `net:net0` dhcp 10.0.0.3.
4. **T26 sola-terminal in.** **T28 sola-browser in** on canto (CEF
   zygote; helper ready). **T29 sola-workspaces + solactl in** on
   canto. **T37** session Steam in; `/bin/sola-arcade` **not** on canto
   (no DRM modifiers). Other kit apps still out. **Sola master**
   merged into oath-sola (`59a54d59`, Sola `0d364617`: compositor
   death exits 1; Quit Sola stays 0). Packed `pkg:sola` on canto:
   kvm libexec still `386c9d78`; rest of the blob may still be
   `a6dd7c12`.
   `/bin/sola-river` and `/bin/sola-shell` wrappers retry while
   `pidof river` is empty or the session is still up. Flower Restart Computer / Shut Down via
   `oath apply --confirm`; Super+Tab counts, notify pile, volume
   spectrum, rounded float CSD, browser omnibox/devtools.
   `pkg:pipewire` on canto: `svc:pipewire` / `wireplumber` /
   `pipewire-pulse` applied gen 40 as `home` (Built-in Audio);
   HDMI not auto-enumerated. System D-Bus is `pkg:bluez`; no
   session bus / MPRIS.
5. **T24 identity locked** (one `pkg:sola` blob, apply/undo). Oath-as-dev-host
   **started**: Workspaces ELF is on canto. **T35** toolchain live.
   **T30** `pkg:grok` packed. **T31** seat `home` locked (uid 1, SSH
   home, sudo ALL, `/lib/oath`, catalog env).
6. **T20 hosting locked**, not implemented.
7. Do not add a third compositor. River is Sola; Hyprland is Omarchy
   (T39). glibc runtime is allowed **only** as `pkg:glibc` (never in
   PID 1). No udevd. No nested Sola/Omarchy process manager. No
   systemd. Do not write a real disk the operator did not name, or
   without `--confirm`.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe|install` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** | **canto (metal)** |
|--|---------------------|-------------------|
| Role | Serial + SSH + virtio-gpu appliance | First metal canary |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` | `ssh` / `scp` / `sftp` `home@canto` (10.0.0.3) |
| Notes | `dev:card0` + gtk Sola menubar if DISPLAY, 1280×800 1:1 (`dev:kbd0` / `dev:mouse0`, no udevd). Virtio: pixman + SW cursor + `LIBGL_ALWAYS_SOFTWARE`. Menubar panels are card-sized (software GL). Window menu + Super+K from current Sola. Launcher Terminal is `/bin/sola-terminal`. Workspaces + `solactl` packed. Guest SSH is `home`. Host SSH keys on up/start. `pkg:pipewire`, dropbear `scp`/`sftp-server`, and `pkg:thoxa` are in seed for the **next** `cargo make build`; the current qcow was not rebuilt with them. NFS client + `svc:backup` also next image. T39 Hyprland + Quickshell + Omarchy packs are in this qcow (8G rootfs; seed session still sola). Manual: `docs/manual/`. | GPT `/dev/sda` ESP+btrfs `@`. Dual Pitcairn (`1002:6810`) via amdgpu `si_support=1`. HDMI `card1` DP-10. **Now:** Philips 221V8L 1920×1080@75. DualUp native 2560×2880 is 30 Hz on HDMI (60 Hz on the LG’s DP). T31 seat `home`. **T39 `session=omarchy`:** Hyprland 0.52.2 as `home` on DP-10 1920×1080@60 (`wayland-1`; River/Sola stopped). After ESP reboot: kernel **7.3.0-rc1 #4**, T39 `/init`, Hyprland pid 362 + omarchy-shell pid 367 (uid 1, restarts=0), `omarchy-bar` 1920×26. Desired exec still has leftover `sola-hypr=1` / `sola-omarchy=1` tokens from the live switch (harmless; T39 `is_seat_svc` drops uid). ESP last-5: oath + boots **12**–8. (uid 1; DRM/evdev/ALSA `0660` root:`home`.) Packed Sola: kvm libexec `386c9d78`; rest may still be `a6dd7c12`. **`pkg:grok`** `/bin/grok` (updater off). **`pkg:git`** `/bin/git`. **`pkg:curl`** `/bin/curl`. **`pkg:pipewire`** this boot (Built-in Audio PCH; WirePlumber `main-embedded`). **`pkg:bluez`** system dbus + bluetoothd; `hci0` powered (BCM20702). No session dbus. **`pkg:thoxa`** `/bin/thoxa` this boot (hand-copied store Thoxa `c42c9a6` session-rc-split; home passwd `/bin/thoxa`; `/etc/shells` lists it; `host:local.env` `SHELL`; sola wrappers default `$SHELL` to `/bin/thoxa`). EFI splash: white mark on black at GOP 1920×1080 (`oath-efi` as BOOTX64). `/bin/sola-workspaces` + `/bin/solactl` packed. Magic Keyboard + Razer Taipan. `net:net0` dhcp 10.0.0.3. Kit fonts: SF Pro Text + Iosevka Term Slab. `/bin/sola-browser` + CEF in `pkg:sola`. **scp/sftp** live (`/bin/scp`, `/bin/sftp-server` in `pkg:dropbear`). Editor: busybox `/bin/vi`. **sola-kvm listen** this boot (UDP 4242; novus peer; libexec hand-copied for Super-up + drop kernel auto-repeat). **T33 backup** this boot: `canto.send` on nas `10.0.0.12:/mnt/alpha/backup/canto` (gen 16, 2056610447 bytes, checksum match); `svc:backup` daily sleeper. NFS modules insmod’d live. **T34** `host:local.timezone` Mountain; Sola clock MDT; `date` UTC. **T35** `/bin/cc` `/bin/rustc` `/bin/cargo` `/bin/cmake` `/bin/ninja` `/bin/pkg-config` (gen 19; Zig 0.16 + rustc 1.98.1; empty `.pc` farm). **T37** `/bin/bash` `/bin/Xwayland` `/bin/steam` + `pkg:mesa` (gen 21; **session Steam** is River `+xwayland` `:0` — user confirmed; Super+Q SIGTERMs X11 class `steam`; 32-bit RADV + dual ICD + wayland 1.26 `wl_fixes`; GpuTopology PITCAIRN; `liboath-peercred` dlmopen copy; CEF `--disable-gpu`; session X `-glamor off`; `CLONE_NEWUSER` EPERM; **gamescope / sola-arcade uninstalled** gen 39 — no DRM modifiers). **T38** ESP: systemd-boot `BOOTX64` timeout 5; live process is **7.3.0-rc1 #4** (this reboot). Slot is 7.3 + `btusb` + unix_floor dbus dirs (`subvol=@`). Last-5: **boot 12** (T39 init), 11, 10, 9, 8. Boot 7 pruned this rotate. **`pkg:mesa`** Debian **26.2.1** + libdrm 2.4.134 + 32-bit libgbm. |

```sh
nix-shell
cargo make build
cargo make probe
cargo make run
```

---

## Locked models

Do not re-litigate without an explicit decision.

- Oath: Linux kernel, own userspace, musl base, own PID 1.
- Catalog `/oath`, ids `kind:name`, `oath` is the only admin surface.
- v0 kinds `host`, `svc`, `snap`, `pkg`, `net`, `ssh`, `dev`. Sibling
  `@gen-N` at `/oath/run/fs`.
- Network: `net:net0` renamed NIC. Default static slirp
  `10.0.2.15/24`. `ipv4=dhcp` via udhcpc. `OATH_BRIDGE` optional.
- SSH: **home** only, dropbear `-w`, **no baked private key**. Host
  keys under `/oath/ssh/`. Owner pubkeys in `ssh:local` →
  `/home/.ssh/authorized_keys`. `pkg:dropbear` ships musl OpenSSH
  `/bin/ssh` (not glibc; not OpenSSH sshd), `/bin/scp`, and
  `/bin/sftp-server`. Guest client `accept-new` host keys.
  Serial is root when the svc is on
  (break-glass); no UART on canto so `svc:serial` is disabled.
  `home` has `sudo` ALL, no password. Unix name `home`, uid 1,
  `HOME=/home`. Login shell is `/bin/thoxa` (listed in `/etc/shells`).
  Root/serial stay `/bin/sh`. Groups: `root` and `home` only. Required env is
  `host:local.env` (PID 1 injects; `/etc/profile` root-owned; not
  `$HOME/.profile`). ESP **initrd `/init` stays PID 1** after chroot.
- Clock: **T34** — system time is UTC (logs, `snap`, backup sidecar
  `…Z`). `host:local.timezone` is POSIX display TZ (seed Mountain);
  seat/`sola-shell` get `TZ`. Not `/etc/localtime`, not `host.env`.
  T33 04:00 Mountain is POSIX TZ **inside** `backup-daily` only.
- Backup: **T33 (partial):** one NFS copy of whole `@` via `btrfs send`
  of a read-only generation; overwrite. `svc:backup` is
  `/lib/oath/backup-daily` at **04:00 US Mountain** (`restart: always`,
  seed off). Crash-consistent + optional pack
  `libexec/oath-backup-quiesce` / `thaw`. Dest
  `10.0.0.12:/mnt/alpha/backup/canto`. Canto live: gen 16 send; daily
  sleeper on. NFS in the next packed initrd.
- Packages: store `/oath/store/pkg/<name>/` (as-built); `/bin` is a symlink farm;
  `busybox`/`btrfs`/`oath`/`dropbear`/`glibc` not removable; `river`,
  `hyprland`, `quickshell`, `omarchy`, `sola`, `grok`, `git`, `curl`, `pipewire`, `thoxa`, `cc`, `rustc`,
  `cmake`, `pkg-config`, `bash`, `xwayland`, `gamescope`, `mesa`, `steam`, `bluez`, `hello`, and `fetchme` are. `pkg.url` wget canary. **T20:** no
  canonical archive; another Oath host’s store is a valid origin. Git
  is not the store. **T30:** `pkg:grok` is catalog-owned (`/bin/grok`);
  Grok does not self-update. `pkg:git`, `pkg:curl`, `pkg:pipewire`, and `pkg:thoxa` packed.
  **T35:** `pkg:cc` / `pkg:rustc` / `pkg:cmake` / `pkg:pkg-config` packed
  on canto (gen 19; official tarballs; no Nix, no rustup).
  **T37:** `pkg:bash` / `pkg:xwayland` / `pkg:mesa` / `pkg:steam`
  packed on canto (gen 21). Session `/bin/steam` is River `+xwayland`.
  `pkg:gamescope` seed `present: false` + `requires.drm_modifiers`;
  apply refuses on SI/virtio. `/bin/sola-arcade` not linked on canto.
  Not in the QEMU image (`cargo make build` never packs Steam /
  Xwayland / mesa / gamescope).
  **T32 (target, not implemented):** a pack is a directory matching
  that layout (no recipe language). Realization id is the content hash
  of the tree. Store becomes `/oath/store/pkg/<name>/<hash>/`. Name is
  a slot; hash is the bits. `desired.hash` is the pin; apply verifies.
  Two runnable at once is still two names.
- Services: PID 1 converges `svc:*` in `wants` order. Ethernet then
  dhcp/sshd, then amdgpu. `svc:serial` parks if there is no UART.
  `svc:sshd` is dropbear; `svc:hold` wants serial; `svc:river` wants
  `svc:seatd`. `svc:hyprland` wants `svc:seatd` (seed off; T39). Sola session: `svc:sola-bus` / `sola-call` / `river` /
  `shell` / `session` (as `home` when enabled). Audio: `svc:pipewire` /
  `wireplumber` / `pipewire-pulse` as `home` (canto gen 40; live PID 1
  needs a `sola-` exec token to drop uid).
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set
  is pixman River plus the Sola menubar (software GL, McMojave
  cursor), **1280×800 1:1** (`virtio-gpu-pci,xres/yres` + gtk
  `zoom-to-fit=off` + `GDK_SCALE=1` + sola-river
  `SOLA_OUTPUT_PICK=preferred`; `OATH_DISPLAY_WIDTH` / `HEIGHT`). Input is
  libinput via libudev-zero (`dev:kbd0` / `dev:mouse0`). No udevd.
  Path fallback in `forks/wlroots`. Metal BOOTX64 is `oath-efi`
  (native GOP, white mark on black); Linux does not paint fb.
- Graphical desk: `host:local.session` is `sola` (default) or
  `omarchy`. Exclusive compositor. T39: `pkg:hyprland` + `svc:hyprland`
  (seed off); `pkg:quickshell` + `pkg:omarchy` + `svc:omarchy-shell`
  (seed off). No UWSM/SDDM/systemd.
- Sola on Oath: PID 1 is the only supervisor. River is `pkg:river` +
  `svc:river`. Session stack is T23 + T25 (`pkg:sola` + `svc:sola-bus` /
  `call` / `river` / `shell` / `session`; graphical stack as `home`). First kit app is T26
  (`/bin/sola-terminal` + tmux in that blob). T28 is `/bin/sola-browser`
  + CEF in the same blob. T29 is `/bin/sola-workspaces` + `solactl`
  in the same blob. **`/bin/sola-kvm`** is the Linux KVM client
  (`svc:sola-kvm listen` as `home`). **T30:** `pkg:grok` is the
  install; Grok does not update Grok; `$GROK_HOME` is `/home/.grok`; not in
  the Sola blob. `pkg:git`, `pkg:curl`, `pkg:pipewire`, and `pkg:thoxa` packed.
  **T35** guest toolchain live on canto. **T24:** one `pkg:sola` blob;
  development versions are apply/undo of the real objects (no second
  PATH, no nested PM). Oath-as-dev-host **started** (T29 Workspaces on
  canto). glibc is sealed
  `pkg:glibc`. `forks/river` +
  `forks/wlroots` + `forks/sola`. Do not run `crates/sola`.
  First-party pkg sources under `apps/` (`hello`, `fetchme`).
  Sola-generic fixes cherry-pick to `moonlight-pm/Sola`, then merge
  back; Oath-compat stays on `oath-sola`. Merge Sola `master` into
  `oath-sola` regularly so the fork does not drift
  ([forks/README.md](forks/README.md)).
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-09-10-omarchy-session.md](docs/specs/2026-09-10-omarchy-session.md)
  (T39 Omarchy session payload; keep Sola; Hyprland compositor).
  [docs/specs/2026-09-06-boot-generations.md](docs/specs/2026-09-06-boot-generations.md)
  (T38 last-5 firmware boots; current kernel/mesa).
  [docs/specs/2026-09-05-arcade-steam.md](docs/specs/2026-09-05-arcade-steam.md)
  (T37 Arcade + Steam runtime).
  [docs/specs/2026-09-05-guest-toolchain.md](docs/specs/2026-09-05-guest-toolchain.md)
  (T35 guest toolchain).
  [docs/specs/2026-09-04-utc-clock.md](docs/specs/2026-09-04-utc-clock.md)
  (T34 UTC system clock; `host:local.timezone` display).
  [docs/specs/2026-09-03-backup-nfs.md](docs/specs/2026-09-03-backup-nfs.md)
  (T33 one NFS copy; partial, canto).
  [docs/specs/2026-09-03-pkg-pack-identity.md](docs/specs/2026-09-03-pkg-pack-identity.md)
  (T32 pack identity; not implemented).
  [docs/specs/2026-09-03-pkg-thoxa.md](docs/specs/2026-09-03-pkg-thoxa.md)
  (`pkg:thoxa` login shell). T31:
  [docs/specs/2026-09-02-seat-home.md](docs/specs/2026-09-02-seat-home.md)
  (T31 seat `home` + `/lib/oath` + env). T30:
  [docs/specs/2026-09-02-pkg-grok.md](docs/specs/2026-09-02-pkg-grok.md)
  (`pkg:grok` identity). T29:
  [docs/specs/2026-09-02-sola-workspaces.md](docs/specs/2026-09-02-sola-workspaces.md)
  (sola-workspaces). T28:
  [docs/specs/2026-09-01-sola-browser.md](docs/specs/2026-09-01-sola-browser.md)
  (sola-browser). T27:
  [docs/specs/2026-08-31-metal-canto.md](docs/specs/2026-08-31-metal-canto.md)
  (metal canary, partial). T26:
  [docs/specs/2026-08-31-sola-terminal.md](docs/specs/2026-08-31-sola-terminal.md)
  (sola-terminal). T25:
  [docs/specs/2026-08-31-sola-session.md](docs/specs/2026-08-31-sola-session.md)
  (session manager). T24:
  [docs/specs/2026-08-31-sola-dev.md](docs/specs/2026-08-31-sola-dev.md)
  (identity; Oath-as-dev-host started). T23:
  [docs/specs/2026-08-30-oath-sola.md](docs/specs/2026-08-30-oath-sola.md)
  (session stack). T22:
  [docs/specs/2026-08-30-libinput.md](docs/specs/2026-08-30-libinput.md)
  (shipped).
- Plan: [docs/plans/2026-09-01-sola-browser-plan.md](docs/plans/2026-09-01-sola-browser-plan.md)
  (T28, complete). T27:
  [docs/plans/2026-08-31-metal-canto-plan.md](docs/plans/2026-08-31-metal-canto-plan.md)
  (complete). T26:
  [docs/plans/2026-08-31-sola-terminal-plan.md](docs/plans/2026-08-31-sola-terminal-plan.md)
  (complete). No T29 plan file (packed from the freeze).
- Hosting: [docs/specs/2026-08-30-pkg-hosting.md](docs/specs/2026-08-30-pkg-hosting.md)
  (T20 identity, not implemented). Pack identity: T32
  ([docs/specs/2026-09-03-pkg-pack-identity.md](docs/specs/2026-09-03-pkg-pack-identity.md))
- Roadmap: display canary in; River as `svc`; Sola session stack +
  session manager as `svc`; sola-terminal packed; sola-browser packed
  (canto; QEMU on next `cargo make build`); sola-workspaces packed
  (canto; QEMU on next build); T31 seat `home` on canto SSH +
  graphical stack as `home`; `pkg:grok` / `pkg:git` / `pkg:curl` / `pkg:pipewire`
  packed; `pkg:thoxa` packed as the `home` login shell; T33 one NFS
  copy on nas (canto gen 16); T35 guest toolchain live on canto (gen 19);
  T37 session Steam on canto (gen 21); gamescope/arcade not on SI; other kit apps not;
  T39 Omarchy session payload (Hyprland + Quickshell packed, Sola default);
  Phase 6 metal canary (canto) dogfoodable
