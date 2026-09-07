**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing + live-install scripts; canto apply gen 21
**Dogfood:** canto `/bin/bash`, `/bin/sola-arcade`, `/bin/Xwayland` (`-version`
  24.1.13), `/bin/gamescope`, `/bin/steam`. 32-bit Steam ELF loads with
  `/lib/ld-linux.so.2`. `/bin/steam` execs past `srt-logger` / `steam.sh`
  (`/usr/bin/env` + `/lib64` loader); canto downloaded and extracted the
  ubuntu12 client (~496 MB). steamwebhelper stays up on the host
  (`STEAM_RUNTIME_STEAMRT` → `pkg:steam/libexec/pv-host`) with `pkg:mesa`
  GLX (CEF `BrowserReady`; WebUITransport accepts via `oath-lsof`). Guest
  `cargo build -p sola-arcade` succeeded. `gamescope --backend wayland`
  selects RADV PITCAIRN and allocates descriptors (pool layer);
  nested Xwayland starts; River accepts the nest window
  (`libdecor-oath`). Direct `/bin/steam` execs gamescope (`--backend
  wayland --steam`, no `-b`, no `--force-windows-fullscreen`; nested
  client is Deck UI `-gamepadui -steamdeck`). Launcher lists Steam.
  32-bit RADV
  loads (`pkg:mesa/lib32` wayland 1.26 `wl_fixes_interface`; ICD
  DT_RPATH so steamrt’s old wayland cannot hide it). GpuTopology
  reports RADV PITCAIRN. `liboath-peercred.so` is first DT_NEEDED of a
  *copy* (`steamui.oath.so`) loaded via interposed `dlmopen` — live
  `steamui.so` stays unpatched so Steam’s verifier does not loop.
  steamwebhelper `_v2-entry-point` adds `--disable-gpu` (CEF GPU
  SIGBUS 135 on RADV SI). steamui library chrome paints after login
  (Deck welcome in the nest). The post-`PopupHTMLWindow` SIGSEGV was
  a NULL `SDL_CreateWindow` from `dlsym(RTLD_NEXT)` in the dlmopen
  NS; `load_sym` uses the real `dlopen`. `/usr/share/X11/locale` is
  live-linked from steam-runtime (`XLOCALEDIR`). Novus runs Steam as a
  host X11 client on River `+xwayland` inside nixpkgs `steam-*-bwrap`;
  not a gamescope session.
**Gaps:** Direct `/bin/steam` nests in gamescope (`--backend wayland`,
  no `-b`; `-b` commits xdg 0×0 and segfaults). Do not pass
  `--force-windows-fullscreen` (0×0 CEF buffer upscaled = static on
  RADV SI). Do not patchelf live `steamui.so`. River accepts the nest
  (`libdecor-oath` 1px). steamui library window paints after login
  (Deck welcome); CEF BrowserReady is up. Nested present to River
  is still wrong: live GET is `tiling_info=0x6016a4`
  (`ARRAY_2D_TILED_THIN1`, THIN, P8_32x32_8x16). Layer no longer
  SETs LINEAR_ALIGNED (that lied). Layer remaps swapchain-sized
  `vkAllocateMemory` to host-visible GTT so CPU detile can `GEM_MMAP`
  (unsmoked this boot). 32-bit `libgbm` is in `pkg:mesa/lib32`.
  steam-runtime `compose.dir` aliases C.UTF-8. Internal
  `gamescopectl screenshot` is the Deck UI. Wrapper forces
  composition and SDR; that is not the remaining glass bug.
  CEF GPU is `--disable-gpu` on SI. WebUITransport
  needs `/bin/lsof` (`oath-lsof`). Arcade Play unsmoked. Rootful `:2`
  + clip/xwm is leftover fallback. QEMU image pack of these pkgs not
  in `cargo make build` yet. Other T36 kit ELFs still out (`alsa.pc`).
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Arcade + Steam runtime (`pkg:bash` / `pkg:xwayland` / `pkg:gamescope` / `pkg:mesa` / `pkg:steam`)

T36 packed the Arcade **ELF** and left Steam/gamescope/XWayland out.
This freeze packs those runtimes as **separate removable `pkg:*`**
and puts `sola-arcade` on `/bin`. Amends T23/T36 Out for this slice
only: the nest is in. River is not rebuilt with host XWayland.

---

## Locks this freeze owns

- **`sola-arcade` stays in the one `pkg:sola` blob** (T24/T36). Store
  `/oath/store/pkg/sola/libexec/sola-arcade`; `/bin/sola-arcade` via
  the farm. Do not add `pkg:sola-arcade`.
- **Steam / gamescope / Xwayland are not the Sola blob.** Removable
  `pkg:steam`, `pkg:gamescope`, `pkg:xwayland`, `pkg:mesa`. Same class as
  `pkg:pipewire`. `pkg:mesa` is the 64-bit X11 GLX stack plus Vulkan WSI
  (Debian mesa; **T38** live 26.2.1 RADV + Khronos loader); River stays GLES/EGL.
- **`pkg:bash` is borrowed static musl GNU bash** (same class as
  `pkg:grok`). Steam’s launcher and Grok’s agent shell need `builtin`
  / `shopt` / `-O extglob`. Busybox ash is not bash. `/bin/bash` is
  the ELF, not a `#!/bin/sh` wrapper.
- **No new `svc:*`.** PID 1 does not supervise Arcade, gamescope, or
  Steam. `sola-session` LaunchApp spawns `/bin/sola-arcade --run …`.
- **Nest is gamescope as a Wayland client** (`--backend wayland`,
  never host `-f`). Host River is still packed with
  `xwaylandSupport = false`. Nested X is gamescope’s `Xwayland` on
  PATH (`/bin/Xwayland`).
- **32-bit glibc lives in `pkg:steam/lib32`**, not a second libc in
  PID 1. The ubuntu12_32 Steam ELF wants `/lib/ld-linux.so.2`; apply
  may symlink that node at the packed loader. Multiarch
  `/lib/i386-linux-gnu` is the loader search path.
- **Steam must not rewrite `pkg:glibc`.** Ubuntu 2.34+ folded
  `libresolv` into libc; this `pkg:glibc` still ships a separate
  `libresolv.so.2` (`__b64_pton@GLIBC_2.2.5`). tmux NEEDs it and
  searches `pkg:glibc` first on rpath. Stubbing `libresolv` →
  `libc.so.6` makes sola-terminal and workspaces fail to open panes.
  srt-logger gets `libresolv` from `pkg:steam/lib/srt` (copy from
  `pkg:sola`). Host nodes (`/usr/bin/env`, `/lib64` loader) are fine.
- **User Steam state is `$HOME/.steam` and `$HOME/.local/share/Steam`.**
  The pack is the launcher + bootstrap tarball, not the library.
- **Canto fill** is `image/install-bash.sh`,
  `image/build-sola-arcade.sh`, `image/install-arcade-runtime.sh`.
  Official / Ubuntu / Debian debs, relocated. No Nix on canto.

---

## Courage test (this slice)

On canto (and QEMU after the image pack):

1. `test -x /bin/bash` and `bash --version` is GNU bash.
2. `test -x /bin/sola-arcade` and `readlink /bin/sola-arcade` is
   `/oath/store/pkg/sola/bin/sola-arcade`.
3. `test -x /bin/Xwayland` and `Xwayland -version` prints 24.1.x.
4. `test -x /bin/gamescope` and `test -x /bin/steam`.
5. Serial and SSH still work. `pgrep -x sola` stays empty.
6. `tmux -V` prints a version (not `undefined symbol: __b64_pton`).
   `pkg:glibc` `libresolv.so.2` is a real DSO, not a `libc.so.6` stub.

---

## Out

- Rebuilding River with `xwaylandSupport` (host X11 Steam windows)
- `dbus-daemon`, a second Unix user, `pkg:python`
- Splitting `pkg:sola`
- Upgrading sealed `pkg:glibc` to Ubuntu questing
- Other remaining T36 kit ELFs (spotify still wants `alsa.pc`)
