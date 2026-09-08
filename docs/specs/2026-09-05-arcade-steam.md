**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing + live-install scripts; canto apply gen 21;
  River packed `+xwayland` (amended 2026-09-08). Gamescope-on-SI
  nest parked 2026-09-08 (`t37-gamescope-canto`); `pkg.requires.drm_modifiers`.
**Dogfood:** canto `/bin/bash`, `/bin/Xwayland` (`-version` 24.1.13),
  `/bin/steam`. 32-bit Steam ELF loads with `/lib/ld-linux.so.2`.
  Session `/bin/steam` is a River `+xwayland` X11 client (`DISPLAY=:0`,
  `WLR_XWAYLAND=/bin/Xwayland`); user confirmed the library paints.
  steamwebhelper stays up on the host (`STEAM_RUNTIME_STEAMRT` →
  `pkg:steam/libexec/pv-host`) with `pkg:mesa` GLX (CEF
  `BrowserReady`; WebUITransport accepts via `oath-lsof`).
  `liboath-peercred.so` is first DT_NEEDED of a *copy*
  (`steamui.oath.so`) loaded via interposed `dlmopen` — live
  `steamui.so` stays unpatched. steamwebhelper `_v2-entry-point`
  adds `--disable-gpu` (CEF GPU SIGBUS 135 on RADV SI). Super+Q
  SIGTERMs X11 class `steam` after WM_DELETE (River `699a166`).
  **`pkg:gamescope` and `/bin/sola-arcade` are not installed on
  canto** (Pitcairn `1002:6810` has no Vulkan WSI DRM modifiers).
  Novus Steam remains nixpkgs `*-bwrap` FHS.
**Gaps:** Other T36 kit ELFs still out (`alsa.pc`). Arcade Play is
  for GPUs with DRM modifiers only (not SI). QEMU never packs
  Steam / Xwayland / mesa / gamescope (metal live-install only).
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Arcade + Steam runtime (`pkg:bash` / `pkg:xwayland` / `pkg:gamescope` / `pkg:mesa` / `pkg:steam`)

T36 packed the Arcade **ELF** and left Steam/gamescope/XWayland out.
This freeze packs those runtimes as **separate removable `pkg:*`**.
**Amended 2026-09-08:** host River is packed `+xwayland`; session
`/bin/steam` is a normal X11 client. Gamescope nest on SI/Pitcairn
is **out** (parked; tag `t37-gamescope-canto`). `pkg:gamescope` and
`/bin/sola-arcade` require Vulkan WSI DRM format modifiers.

---

## Locks this freeze owns

- **`sola-arcade` stays in the one `pkg:sola` blob** (T24/T36). Store
  `/oath/store/pkg/sola/libexec/sola-arcade`. `/bin/sola-arcade` is
  linked only when the connected GPU has DRM modifiers. Do not add
  `pkg:sola-arcade`. Do not install Arcade on SI/Pitcairn (canto).
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
- **`pkg:gamescope` needs DRM modifiers.** Desired
  `requires.drm_modifiers: true`. Apply `present=true` is refused
  unless a connected GPU exports Vulkan WSI DRM format modifiers
  (AMD GFX6–8 and virtio-gpu do not). Seed `present: false`.
  Uninstall is always allowed. Same probe skips the `sola-arcade`
  farm link (do not refuse `pkg:sola`).
- **Arcade Play nest is gamescope as a Wayland client** on GPUs
  that pass that check (`--backend wayland`, never host `-f`).
  Nested X is gamescope’s `Xwayland` on PATH. Not on canto.
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
  `image/install-arcade-runtime.sh` (Steam + Xwayland + mesa;
  gamescope/arcade skipped on this GPU). Official / Ubuntu / Debian
  debs, relocated. No Nix on canto.
- **Not the QEMU image.** `cargo make build` does not pack
  `pkg:steam` / `pkg:xwayland` / `pkg:mesa` / `pkg:gamescope`.
  The appliance is not a Steam host. Novus desktop Steam stays
  nixpkgs `*-bwrap`.

---

## Courage test (this slice)

On canto:

1. `test -x /bin/bash` and `bash --version` is GNU bash.
2. `test -x /bin/Xwayland` and `Xwayland -version` prints 24.1.x.
3. `test -x /bin/steam`. Direct `/bin/steam` is an X11 client on
   session `:0`.
4. On canto (Pitcairn): `test ! -e /bin/gamescope` and
   `test ! -e /bin/sola-arcade`. `oath apply pkg:gamescope` with
   `present=true` is refused (DRM modifiers).
5. Serial and SSH still work. `pgrep -x sola` stays empty.
6. `tmux -V` prints a version (not `undefined symbol: __b64_pton`).
   `pkg:glibc` `libresolv.so.2` is a real DSO, not a `libc.so.6` stub.

---

## Out

- Library Steam inside gamescope (parked 2026-09-08; session X11 instead)
- Gamescope nest on SI/Pitcairn (parked 2026-09-08; tag
  `t37-gamescope-canto`). No RADV LINEAR_ALIGNED export, no
  `VK_LAYER_OATH_gamescope_pool`, no `libdecor-oath`, no mesa SI
  nest patches.
- `dbus-daemon`, a second Unix user, `pkg:python`
- Splitting `pkg:sola`
- Upgrading sealed `pkg:glibc` to Ubuntu questing
- QEMU image pack of Steam / Xwayland / mesa / gamescope
- Other remaining T36 kit ELFs (spotify still wants `alsa.pc`)
