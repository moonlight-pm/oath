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
  (`libdecor-oath`).
**Gaps:** `gamescope --backend wayland` selects RADV PITCAIRN,
  allocates descriptors, starts nested Xwayland, and River accepts
  the nest window (`libdecor-oath` 1px borders). Packed
  `libdecor-cairo` mmap-crashes on a 0-size CSD buffer so dummy
  wins. Direct `steam` nests in gamescope (`--backend wayland`, no
  `-b`). CEF GPU uses `pkg:mesa`
  GLX (`BrowserReady`). WebUITransport accepts the loopback websocket
  once `/bin/lsof` exists (`oath-lsof`: `lsof -P -F upnR -i TCP@…`).
  Login popup is up on canto `:2`. Rootful Xwayland CLIPBOARD is
  bridged from the compositor (`xwayland-clip`: `wl-paste --watch` →
  `xclip`) so Ctrl+V pastes (canto this boot). After login the library
  window was created at INT_MIN (no X11 WM on rootful `:2`) and Steam
  segfaulted; `oath-xwm` maps/clamps windows on `:2`. 32-bit RADV has
  `libdisplay-info.so.3` + `libxml2.so.16`; dual ICD jsons. Steam
  still logs missing `VK_KHR_surface` / `VK_KHR_xlib_surface`. Arcade
  Play unsmoked. QEMU image pack of these pkgs not in `cargo make
  build` yet. Other T36 kit ELFs still out (`alsa.pc`).
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
  (Debian 26.1.6 RADV + Khronos loader); River stays GLES/EGL.
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
