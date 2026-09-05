**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing + live-install scripts; canto apply gen 21
**Dogfood:** canto `/bin/bash`, `/bin/sola-arcade`, `/bin/Xwayland` (`-version`
  24.1.13), `/bin/gamescope`, `/bin/steam`. 32-bit Steam ELF loads with
  `/lib/ld-linux.so.2`. `/bin/steam` execs past `srt-logger` / `steam.sh`
  (`/usr/bin/env` + `/lib64` loader); canto downloaded and extracted the
  ubuntu12 client (~496 MB). steamwebhelper stays up on the host
  (`STEAM_RUNTIME_STEAMRT` → `pkg:steam/libexec/pv-host`). Guest
  `cargo build -p sola-arcade` succeeded.
**Gaps:** `gamescope --backend wayland` selects RADV PITCAIRN then
  dies (`vkAllocateDescriptorSets` / `VK_KHR_wayland_surface`). Steam
  uses rootful `pkg:xwayland` (`DISPLAY=:2`). steamwebhelper skips
  pressure-vessel (`STEAM_RUNTIME_STEAMRT` → `pkg:steam/libexec/pv-host`;
  `CLONE_NEWUSER` is EPERM after PID 1 chroot) and stays up; CEF GPU
  process still SIGSEGV (no `libGLX_mesa`; 64-bit helpers still hit
  ELFCLASS32 `libGL` from ubuntu12_32). QEMU image pack of these pkgs
  not in `cargo make build` yet. Other T36 kit ELFs still out (`alsa.pc`).
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Arcade + Steam runtime (`pkg:bash` / `pkg:xwayland` / `pkg:gamescope` / `pkg:steam`)

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
  `pkg:steam`, `pkg:gamescope`, `pkg:xwayland`. Same class as
  `pkg:pipewire`.
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

---

## Out

- Rebuilding River with `xwaylandSupport` (host X11 Steam windows)
- `dbus-daemon`, a second Unix user, `pkg:python`
- Splitting `pkg:sola`
- Upgrading sealed `pkg:glibc` to Ubuntu questing
- Other remaining T36 kit ELFs (spotify still wants `alsa.pc`)
