**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing + live-install scripts; canto apply gen 21;
  River packed `+xwayland` (amended 2026-09-08)
**Dogfood:** canto `/bin/bash`, `/bin/sola-arcade`, `/bin/Xwayland`
  (`-version` 24.1.13), `/bin/gamescope`, `/bin/steam`. 32-bit Steam
  ELF loads with `/lib/ld-linux.so.2`. Session `/bin/steam` is a
  River `+xwayland` X11 client (`DISPLAY=:0`,
  `WLR_XWAYLAND=/bin/Xwayland`); user confirmed the library paints.
  steamwebhelper stays up on the host (`STEAM_RUNTIME_STEAMRT` →
  `pkg:steam/libexec/pv-host`) with `pkg:mesa` GLX (CEF
  `BrowserReady`; WebUITransport accepts via `oath-lsof`). Guest
  `cargo build -p sola-arcade` succeeded. Arcade lucide loads from
  `/oath/store/pkg/sola/share` when `/oath/INDEX.md` exists
  (oath-sola `53ca8c10`). Arcade Play still nests
  `gamescope --backend wayland` (wrapper drops `-b`; nested X
  usually `:1`, `-glamor off`). 32-bit RADV loads
  (`pkg:mesa/lib32` wayland 1.26 `wl_fixes_interface`; ICD DT_RPATH).
  GpuTopology reports RADV PITCAIRN. `liboath-peercred.so` is first
  DT_NEEDED of a *copy* (`steamui.oath.so`) loaded via interposed
  `dlmopen` — live `steamui.so` stays unpatched. steamwebhelper
  `_v2-entry-point` adds `--disable-gpu` (CEF GPU SIGBUS 135 on
  RADV SI). Super+Q SIGTERMs X11 class `steam` after WM_DELETE
  (River `699a166`). Novus Steam remains nixpkgs `*-bwrap` FHS.
**Gaps:** Arcade Play unsmoked (refuses while `ubuntu12_32/steam`
  is live; leftover client was killed; Super+Q reap not
  user-verified). Library-in-gamescope parked (CEF ~1 fps, strobe,
  xdg-never-configured). Do not `--force-windows-fullscreen`. Do
  not pin Deck 1280×800 for titles. Do not patchelf live
  `steamui.so`. Nested Xwayland is `-glamor off`. Live canto RADV
  linear-export is WSI/external **and large TRANSFER_SRC**; tree
  `0002` is sampled/WSI only (emptied the nest). radeonsi keeps
  imported pitch on INVALID dmabufs (`0001`). Dual Pitcairn: nest
  must use the connected card (`OATH_DRM_RENDER`). CEF is
  `--disable-gpu` on SI. QEMU image pack of these pkgs not in
  `cargo make build` yet. Other T36 kit ELFs still out (`alsa.pc`).
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Arcade + Steam runtime (`pkg:bash` / `pkg:xwayland` / `pkg:gamescope` / `pkg:mesa` / `pkg:steam`)

T36 packed the Arcade **ELF** and left Steam/gamescope/XWayland out.
This freeze packs those runtimes as **separate removable `pkg:*`**
and puts `sola-arcade` on `/bin`. Amends T23/T36 Out for this slice
only: the nest is in. **Amended 2026-09-08:** host River is packed
`+xwayland`; session `/bin/steam` is a normal X11 client. Arcade
Play still uses the gamescope nest.

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
- **Arcade Play nest is gamescope as a Wayland client**
  (`--backend wayland`, never host `-f`). Nested X is gamescope’s
  `Xwayland` on PATH (`/bin/Xwayland`), usually `:1`, `-glamor off`.
- **Session Steam is host X11.** Host River is packed with
  `xwaylandSupport = true` and `WLR_XWAYLAND=/bin/Xwayland`.
  `/bin/steam` is a River Xwayland client on `:0`. Do not exec
  gamescope for the library. Super+Q / Sola close SIGTERMs X11
  class `steam` after WM_DELETE (`STEAM_FRAME_FORCE_CLOSE=1` is
  not enough).
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
7. Direct `/bin/steam` is an X11 client on session `:0` (not nested in
   gamescope). Arcade Play still uses the gamescope nest.

---

## Out

- Library Steam inside gamescope (parked 2026-09-08; session X11 instead)
- `dbus-daemon`, a second Unix user, `pkg:python`
- Splitting `pkg:sola`
- Upgrading sealed `pkg:glibc` to Ubuntu questing
- Other remaining T36 kit ELFs (spotify still wants `alsa.pc`)
