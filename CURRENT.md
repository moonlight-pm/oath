# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-09-07

---

## Now

1. **T31 on canto: `ssh home@canto`.** uid 1, `HOME=/home`, sudo ALL
   no password, groups `root`+`home` only. Graphical stack **on**
   as `home` (River GLES2/radeonsi + Sola on amdgpu DP-10, Philips
   1920×1080). `/dev/ptmx` is 0666 so terminal tmux/PTY works.
   Serial svc **off** (no UART). EFI mark still paints. **T30**
   `pkg:grok` packed (`/bin/grok`, updater off). Guest `/bin/xdg-open`
   is `solactl open` so Grok login can raise sola-browser. `pkg:git` and
   `pkg:curl` packed. Menubar CPU/MEM/RX LED graphs paint (image
   raster, not 1×1 canvas). Volume chip is up (`pkg:pipewire` +
   ALSA HDA; Built-in Audio). **scp / sftp** as `home@canto` (dropbear
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
   tarballs, no Nix, no rustup. **T37** Arcade + Steam runtime on
   canto (gen 21): `/bin/bash` (GNU 5.2.15 static musl),
   `/bin/sola-arcade` (guest cargo), `/bin/Xwayland` 24.1.13,
   `/bin/gamescope`, `/bin/steam` (bootstrap extracts; 32-bit ELF
   loads; launcher execs; ubuntu12 client on disk; `steamui.so`
   loads with 32-bit GL from steamrt3c). Direct `/bin/steam` execs
   **gamescope** (`--backend wayland --steam`, no `-b`, no
   `--force-windows-fullscreen`: that stretched Steam’s 0×0 offscreen
   CEF buffer to 1920×1080 and looked like GPU static). Nested
   Xwayland is gamescope’s `-rootless`
   `:0`. River floats the nest 1920×1052. steamwebhelper is host
   `_v2-entry-point` (`CLONE_NEWUSER` EPERM). Launcher lists **Steam**
   (`~/.config/sola/shell/applications.json` + `state.yaml`
   `Application`; `/bin/steam`; `lucide/gamepad-2`). **`pkg:mesa`**
   is GLX plus Vulkan WSI (RADV 26.2.1). 32-bit RADV loads (Debian
   `libwayland-client` 1.26 in `pkg:mesa/lib32` for
   `wl_fixes_interface`; steamrt 0.3.0 is too old; ICD uses DT_RPATH).
   GpuTopology reports RADV PITCAIRN. Rootful `:2` +
   `xwayland-clip` / `oath-xwm` is leftover fallback only.
   `liboath-peercred.so` is in the steamui **dlmopen** namespace via a
   patched copy (`ubuntu12_32/steamui.oath.so`); do **not** patchelf
   live `steamui.so` (Steam verifies size and re-extracts forever).
   steamwebhelper `_v2-entry-point` passes `--disable-gpu` (CEF GPU
   process SIGBUS 135 on RADV SI). steamui **library paints** after
   login (Deck welcome “PRESS any button to continue”; 1920×1080
   depth-32 X window). The post-`PopupHTMLWindow` SIGSEGV was
   `dlsym(RTLD_NEXT, SDL_CreateWindow)` NULL in the steamui dlmopen
   NS (shim is first DT_NEEDED); `load_sym` dlopens `libSDL3.so.0`
   via constructor-saved real `dlopen`. Missing
   `/usr/share/X11/locale` was an earlier NULL-deref
   (`_XlcCreateLocaleDataBase`); steam-runtime locale is live-linked
   and `XLOCALEDIR` is set. Not missing `VK_KHR_surface`, not SDL
   modal, not live-file DT_NEEDED. **Gap:** nest present. Nested
   Xwayland is **`-glamor off`** (river radeonsi SIGBUS’d on SI tiled
   BOs; mesa 26.2.1 `libgallium` needs `acosf@GLIBC_2.43` and
   pkg:glibc is 2.42). 64-bit `liboath-glclass.so` redirects
   `libGL`/`libEGL` dlopen to `pkg:mesa` so `ubuntu12_64/gldriverquery`
   is no longer ELFCLASS32 (it now fails “matching GLX visual” with
   glamor off). `pkg:mesa` also ships 64-bit EGL. Do **not** patchelf
   live `steamui.so` (steam-compat no longer `--add-needed`s it).
   2026-09-07 evening smoke: GpuTopology RADV PITCAIRN; pool layer
   `vkCreateImage` 1920×1080; `SDL_CreateWindow` OpenGL 4.5; **no
   SIGBUS**. `xdg_surface#19: error 3: xdg_surface has never been
   configured` was River skipping the first configure because
   `svc:sola-river` was down (compositor restart → bridge exited 0
   → `restart=on-failure` left it stopped). Nest holds with the WM
   up (`propose_dimensions` 1920×1080). Wrappers retry while `pidof
   river` is empty (this boot, gen 28). Do **not** SET
   `LINEAR_ALIGNED` on the tiled source. 32-bit `libgbm` is in
   `pkg:mesa/lib32`. steam-runtime `compose.dir` aliases C.UTF-8.
   Arcade Play unsmoked.
   Super+Tab focuses the Gamescope nest. Novus is
   not a gamescope Steam session: River is
   `+xwayland`, `steam` is nixpkgs `*-bwrap` FHS, `unshare -U`
   works, dbus is up; Steam is a host X11 window. **tmux:** new
   Terminal tabs and workspaces splits work (do not stub `pkg:glibc`
   `libresolv`). A second sola-terminal process used to retract the
   first's new tabs (Sola-generic fix packed this boot). **T36** other
   kit names still out (`alsa.pc`). Zig `cc` host link is
   `image/oath-cc-link.sh` + `zig-gnu-cc.sh`. Next: Arcade Play
   (library empty); `/bin/gamescope` drops `-b`. Or a `.pc` for alsa.
   `lo` is up this boot (`127.0.0.1`); PID 1 `unix_floor` will do that
   on the next image.
   **T38** firmware boots on canto ESP: last five archives + current.
   `oath-efi` menu (metal timeout 5, QEMU EFI 0). PID 1 honors
   `oath.subvol=@` / `@boot-N` / `@gen-N`. Ubuntu mainline 7.3-rc1
   **panicked** on canto (PID 1 died after dhcp). Kernel lock amended:
   vanilla kernel.org **7.3.0-rc1** compiled on the desk
   (`image/build-linux.sh`, nice 10, `-j16`; `joshua@novus`, gcc 14.3).
   ESP **Oath** default is 7.3 DPM timings + RunningOnAC (`subvol=@`).
   **Oath boot 4** is the 6.12 rescue. **boot 7** is previous 7.3 DPM #2.
   **boot 6** is 7.3 DC_SI #2 (pre-DPM-patch). **boot 1** pruned this
   rotate. systemd-boot stays `BOOTX64` so the 5 s list is visible
   (oath-efi logo still hides the picker). Live process is **7.3.0-rc1
   #3** + GPIO_DC-skip `amdgpu`. `force high` is **850/1270 MHz**
   (level 3 of 4). VBIOS `HARDWAREDC` was floating the DC GPIO so the
   SMC clamped to 300/150 and `SetForcedLevels` returned 0xff. `pkg:mesa` live is Debian **26.2.1**
   plus **libdrm 2.4.134** and **32-bit libgbm**. card1 DP-10 is up.
   Small SI DPM patches live under `image/linux-patches` (vanilla
   tarball + fragment + patches; not a linux.git fork). **`cargo make
   boot` must pack Pitcairn firmware** (`build/linux/firmware` or
   `OATH_FIRMWARE`); the first DPM initrd omitted it and both GPUs
   `Fatal error during GPU init` (`pitcairn_mc.bin` missing). Live
   recovery: copy firmware + `insmod`. ESP initrd now includes the
   blobs. DPM: skip `GPIO_DC` when `ac_power`; `force high` **850/1270**.
   `gpu_busy_percent` is ENOTSUP. Gamescope pool layer **smoked**: host-visible GTT +
   CPU detile; nested Xwayland is `-glamor off` (no SIGBUS); nest
   holds with sola-river up (xdg never-configured was WM-down, not
   SIGBUS). Steam-runtime `compose.dir` already aliases C.UTF-8.
   Spare GPU idea: `docs/ideas/2026-09-07-canto-second-pitcairn.md`.
   Do not `cargo make install --confirm` (wipe).
2. **T27 metal canary is in.** `ssh home@canto`. `host:local` canto,
   `net:net0` dhcp 10.0.0.3.
3. **T26 sola-terminal in.** **T28 sola-browser in** on canto (CEF
   zygote; helper ready). **T29 sola-workspaces + solactl in** on
   canto. **T37** `/bin/sola-arcade` in. Other kit apps still out. **Sola master**
   merged into oath-sola (`59a54d59`, Sola `0d364617`: compositor
   death exits 1; Quit Sola stays 0). Packed `pkg:sola` on canto is
   still the earlier blob (`a6dd7c12` / kvm hand-copy `386c9d78`);
   `/bin/sola-river` and `/bin/sola-shell` wrappers this boot retry
   while `pidof river` is empty. Flower Restart Computer / Shut Down via
   `oath apply --confirm`; Super+Tab counts, notify pile, volume
   spectrum, rounded float CSD, browser omnibox/devtools.
   `pkg:pipewire` is on canto this boot (PipeWire + WirePlumber +
   pulse + ALSA PCH sink); HDMI not auto-enumerated; no dbus.
4. **T24 identity locked** (one `pkg:sola` blob, apply/undo). Oath-as-dev-host
   **started**: Workspaces ELF is on canto. **T35** toolchain live.
   **T30** `pkg:grok` packed. **T31** seat `home` locked (uid 1, SSH
   home, sudo ALL, `/lib/oath`, catalog env).
5. **T20 hosting locked**, not implemented.
6. Do not add a throwaway compositor. glibc runtime is allowed
   **only** as `pkg:glibc` for this payload (never in PID 1). No
   udevd. No nested Sola process manager. Do not write a real disk
   the operator did not name, or without `--confirm`.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe|install` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** | **canto (metal)** |
|--|---------------------|-------------------|
| Role | Serial + SSH + virtio-gpu appliance | First metal canary |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` | `ssh` / `scp` / `sftp` `home@canto` (10.0.0.3) |
| Notes | `dev:card0` + gtk Sola menubar if DISPLAY, 1280×800 1:1 (`dev:kbd0` / `dev:mouse0`, no udevd). Virtio: pixman + SW cursor + `LIBGL_ALWAYS_SOFTWARE`. Menubar panels are card-sized (software GL). Window menu + Super+K from current Sola. Launcher Terminal is `/bin/sola-terminal`. Workspaces + `solactl` packed. Guest SSH is `home`. Host SSH keys on up/start. `pkg:pipewire`, dropbear `scp`/`sftp-server`, and `pkg:thoxa` are in seed for the **next** `cargo make build`; the current qcow was not rebuilt with them. NFS client + `svc:backup` also next image. Manual: `docs/manual/`. | GPT `/dev/sda` ESP+btrfs `@`. Dual Pitcairn (`1002:6810`) via amdgpu `si_support=1`. HDMI `card1` DP-10. **Now:** Philips 221V8L 1920×1080@75. DualUp native 2560×2880 is 30 Hz on HDMI (60 Hz on the LG’s DP). T31 seat `home`. River/Sola **as `home`** (uid 1; DRM/evdev/ALSA `0660` root:`home`; River GLES2/radeonsi). Packed Sola is oath-sola `a6dd7c12` (LED graphs rastered to an image). **`pkg:grok`** `/bin/grok` (updater off). **`pkg:git`** `/bin/git`. **`pkg:curl`** `/bin/curl`. **`pkg:pipewire`** this boot (Built-in Audio PCH; WirePlumber `main-embedded`; no dbus). **`pkg:thoxa`** `/bin/thoxa` this boot (hand-copied store Thoxa `c42c9a6` session-rc-split; home passwd `/bin/thoxa`; `/etc/shells` lists it; `host:local.env` `SHELL`; sola wrappers default `$SHELL` to `/bin/thoxa`). EFI splash: white mark on black at GOP 1920×1080 (`oath-efi` as BOOTX64). `/bin/sola-workspaces` + `/bin/solactl` packed. Magic Keyboard + Razer Taipan. `net:net0` dhcp 10.0.0.3. Kit fonts: SF Pro Text + Iosevka Term Slab. `/bin/sola-browser` + CEF in `pkg:sola`. **scp/sftp** live (`/bin/scp`, `/bin/sftp-server` in `pkg:dropbear`). Editor: busybox `/bin/vi`. **sola-kvm listen** this boot (UDP 4242; novus peer; libexec hand-copied for Super-up + drop kernel auto-repeat). **T33 backup** this boot: `canto.send` on nas `10.0.0.12:/mnt/alpha/backup/canto` (gen 16, 2056610447 bytes, checksum match); `svc:backup` daily sleeper. NFS modules insmod’d live. **T34** `host:local.timezone` Mountain; Sola clock MDT; `date` UTC. **T35** `/bin/cc` `/bin/rustc` `/bin/cargo` `/bin/cmake` `/bin/ninja` `/bin/pkg-config` (gen 19; Zig 0.16 + rustc 1.98.1; empty `.pc` farm). **T37** `/bin/bash` `/bin/sola-arcade` `/bin/Xwayland` `/bin/gamescope` `/bin/steam` + `pkg:mesa` (gen 21; `/bin/steam` nests in gamescope `--steam`, no `-b`, no `--force-windows-fullscreen`; Deck UI `-gamepadui -steamdeck`; launcher Steam; 32-bit RADV + dual ICD + wayland 1.26 `wl_fixes`; GpuTopology PITCAIRN; `liboath-peercred` dlmopen copy so steamui sees the shim; `load_sym` (RTLD_NEXT is NULL in that NS); CEF `--disable-gpu` (SI SIGBUS); steamui library painted after login on earlier boots (Deck welcome); 2026-09-07 nest smoke: GTT CPU detile; `-glamor off` (no SIGBUS); xdg never-configured was sola-river down (now supervised; nest holds); do not SET LINEAR_ALIGNED; `libdecor-oath` 1px borders; `CLONE_NEWUSER` EPERM from PID 1 chroot). **T38** ESP: systemd-boot `BOOTX64` timeout 5; current is compiled vanilla **7.3.0-rc1 #2** + patched `amdgpu` + Pitcairn firmware in initrd (`subvol=@`). Archives: boot 1 (pre-T38), 2, 4 (6.12 rescue), 5 (7.3 no DC_SI), 6 (7.3 DC_SI #2). **`pkg:mesa`** Debian **26.2.1** + libdrm 2.4.134 + 32-bit libgbm. Steam nest smoke: GTT CPU detile; `-glamor off` (no SIGBUS); xdg never-configured was sola-river down (now supervised; nest holds). |

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
  `sola`, `grok`, `git`, `curl`, `pipewire`, `thoxa`, `cc`, `rustc`,
  `cmake`, `pkg-config`, `bash`, `xwayland`, `gamescope`, `mesa`, `steam`, `hello`, and `fetchme` are. `pkg.url` wget canary. **T20:** no
  canonical archive; another Oath host’s store is a valid origin. Git
  is not the store. **T30:** `pkg:grok` is catalog-owned (`/bin/grok`);
  Grok does not self-update. `pkg:git`, `pkg:curl`, `pkg:pipewire`, and `pkg:thoxa` packed.
  **T35:** `pkg:cc` / `pkg:rustc` / `pkg:cmake` / `pkg:pkg-config` packed
  on canto (gen 19; official tarballs; no Nix, no rustup).
  **T37:** `pkg:bash` / `pkg:xwayland` / `pkg:gamescope` / `pkg:mesa` /
  `pkg:steam` packed on canto (gen 21); `sola-arcade` in `pkg:sola`.
  **T32 (target, not implemented):** a pack is a directory matching
  that layout (no recipe language). Realization id is the content hash
  of the tree. Store becomes `/oath/store/pkg/<name>/<hash>/`. Name is
  a slot; hash is the bits. `desired.hash` is the pin; apply verifies.
  Two runnable at once is still two names.
- Services: PID 1 converges `svc:*` in `wants` order. Ethernet then
  dhcp/sshd, then amdgpu. `svc:serial` parks if there is no UART.
  `svc:sshd` is dropbear; `svc:hold` wants serial; `svc:river` wants
  `svc:seatd`. Sola session: `svc:sola-bus` / `sola-call` / `river` /
  `shell` / `session` (as `home` when enabled). Audio: `svc:pipewire` /
  `wireplumber` / `pipewire-pulse` as `home` (seeded; canto this boot
  started them by hand until ESP initrd is rebuilt).
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set
  is pixman River plus the Sola menubar (software GL, McMojave
  cursor), **1280×800 1:1** (`virtio-gpu-pci,xres/yres` + gtk
  `zoom-to-fit=off` + `GDK_SCALE=1` + sola-river
  `SOLA_OUTPUT_PICK=preferred`; `OATH_DISPLAY_WIDTH` / `HEIGHT`). Input is
  libinput via libudev-zero (`dev:kbd0` / `dev:mouse0`). No udevd.
  Path fallback in `forks/wlroots`. Metal BOOTX64 is `oath-efi`
  (native GOP, white mark on black); Linux does not paint fb.
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
- Freeze: [docs/specs/2026-09-06-boot-generations.md](docs/specs/2026-09-06-boot-generations.md)
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
  T37 Arcade + Steam runtime on canto (gen 21); other kit apps not;
  Phase 6 metal canary (canto) dogfoodable
