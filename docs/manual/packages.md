# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/<hash>/`. `/bin` is a **symlink farm**, not an unpack
target. Name is a slot; hash is the bits (`sha256-` plus 64 hex of
`oath-tree-v1`). The slot also has `live` → `<hash>` and compat
`bin` → `live/bin` so paths like `/oath/store/pkg/<name>/lib` still
work. There is no `apt` and no `oath install`.

## What ships

| Id | Default | Removable |
|----|---------|-----------|
| `pkg:busybox` | `present: true` | no — applets are this one object (includes `/bin/vi`) |
| `pkg:btrfs` | `present: true` | no |
| `pkg:oath` | `present: true` | no |
| `pkg:dropbear` | `present: true` | no — dropbear, dropbearkey, musl OpenSSH `/bin/ssh` + `ssh-keygen`, dropbear `dbclient`/`scp`, sftp-server |
| `pkg:glibc` | `present: true` | no — GNU C runtime for River and Sola; not loaded by PID 1. `libresolv.so.2` is a real DSO (tmux `__b64_pton`); Steam must not stub it to `libc.so.6`. `libutil.so.1` → `libc.so.6` (sola-terminal DT_NEEDED). |
| `pkg:river` | `present: true` | yes — patched River; `/bin/river`. Packed `+xwayland` (`WLR_XWAYLAND=/bin/Xwayland`). Super+Q / Sola close SIGTERMs X11 class `steam` after WM_DELETE (hide-to-tray otherwise). |
| `pkg:hyprland` | `present: true` | yes — nixpkgs Hyprland 0.52.2, no systemd; `/bin/hyprland`. Omarchy compositor. Seed `svc:hyprland` off. |
| `pkg:quickshell` | `present: true` | yes — nixpkgs Quickshell 0.2.1 + Qt 6; `/bin/quickshell`. Omarchy bar. |
| `pkg:omarchy` | `present: true` | yes — Omarchy **v4.0.3** (`0534987`; latest release; `version` file still `4.0.0.alpha`) at `$OMARCHY_PATH` (`/oath/store/pkg/omarchy`). Scripts + QML shell + themes. Desk fonts: JetBrainsMono NF, Liberation, Noto Color Emoji, `omarchy.ttf`. Cursor: **Yaru** XCursor (Omarchy ISO default). Pack-time adapt: `uwsm-app` / `gtk-launch` / `xdg-terminal-exec` / tiny `jq`; `omarchy-pkg-present` checks catalog `pkg:*` (no pacman). Desktop files for sola-terminal, sola-browser, grok. |
| `pkg:sola` | `present: true` | yes — session stack + `sola-terminal` + `sola-browser` + `sola-workspaces` + `sola-kvm` + **`sola-paint`** / **`sola-preview`**; `/bin/sola-bus` and siblings including `sola-session`, `sola-terminal`, `sola-browser`, `sola-workspaces`, `solactl`, `sola-kvm`, `tmux`, `/bin/xdg-open` and `/bin/open` (`solactl open` → sola-browser or sola-paint for images; `x-www-browser` is the same shim; not xdg-utils). C.UTF-8 locale-archive; CEF under `cef/`; SF Pro Text + Iosevka Term Slab, with Inter / JetBrains Mono fallbacks. **`sola-arcade`** stays in this blob; `/bin/sola-arcade` is linked only when the connected GPU has Vulkan WSI DRM modifiers (not on canto Pitcairn). Remaining T36 names (`sola-settings`, mail, spotify, wrapper, …) still not in `/bin` on canto. |
| `pkg:grok` | `present: true` | yes — borrowed static-pie Grok ELF; `/bin/grok`. Updater off (`GROK_DISABLE_AUTOUPDATER`). State is `/home/.grok`, not the payload. |
| `pkg:git` | `present: true` | yes — borrowed Git; `/bin/git`. HTTPS via `git-remote-http` + CA bundle in the store. |
| `pkg:curl` | `present: true` | yes — borrowed static musl curl; `/bin/curl`. CA bundle in the store. |
| `pkg:pipewire` | `present: true` | yes — PipeWire + WirePlumber + pipewire-pulse + alsa-lib + libpulse; `/bin/pipewire`, `/bin/wireplumber`, `/bin/pipewire-pulse`, `/bin/wpctl`, `/bin/pw-dump`, `/bin/pw-cat`. Menubar volume talks to this (`pw-dump` / `wpctl` / `pw-cat`). sola-spotify (not packed) needs the Pulse socket + those libs at runtime. |
| `pkg:bluez` | `present: true` | yes — system D-Bus + BlueZ. `/bin/dbus-daemon`, `/bin/bluetoothd`, `/bin/bluetoothctl`. Menubar Bluetooth chip talks to `org.bluez` on `/run/dbus/system_bus_socket`. No session bus / MPRIS. |
| `pkg:thoxa` | `present: true` | yes — sister compiler + session REPL; `/bin/thoxa` is the `home` login shell (wrapper + `libexec/thoxa`) and the `$SHELL` for sola-terminal / workspaces tmux. Interactive TTY is an emacs line editor with Tab complete (Thoxa `c42c9a6`, canto this boot). Seat rc is `~/.config/thoxa/shell.thx` — do not copy a NixOS `path()` list (`path` *replaces* PATH; import `std/prompt`, not `std/Prompt`). `#` comments may contain apostrophes. Root/serial stay busybox `/bin/sh`. `thoxa -c` is libtcc (no guest `cc`); `thoxa build` needs `pkg:cc`. |
| `pkg:cc` | `present: true` | yes — C toolchain (official Zig 0.16). `/bin/cc` / `c++` / `gcc` / `g++` target glibc (interp + rpath `pkg:glibc`); `/bin/musl-cc` targets musl; `/bin/ar`, `/bin/ranlib`, `/bin/patchelf`. rustc host link uses `image/oath-cc-link.sh` + `zig-gnu-cc.sh` (drop `-fuse-ld=lld`, cc-rs `--target=`, `--dynamic-linker` on `-c`). |
| `pkg:rustc` | `present: true` | yes — official rustc + cargo 1.98.1 (gnu host) + rust-std musl. No rustup. |
| `pkg:cmake` | `present: true` | yes — Kitware cmake 4.3.5 + ninja 1.13.2. |
| `pkg:pkg-config` | `present: true` | yes — `/bin/pkg-config` exists; empty `.pc` farm (`--exists` fails). |
| `pkg:bash` | `present: true` | yes — borrowed static musl GNU bash 5.2.15; `/bin/bash` is the ELF (not an ash wrapper). Steam scripts and Grok’s agent shell need it. |
| `pkg:foot` | `present: true` | yes — Wayland terminal (nixpkgs foot relocated). Omarchy default; Super+Return via `xdg-terminal-exec`. Uses the Omarchy fontconfig file (JetBrainsMono NF). |
| `pkg:grim` | `present: true` | yes — Omarchy capture tools: grim, slurp, hyprpicker, wl-copy, wl-paste, and a real jq. Print Screen → `omarchy-capture-screenshot`. |
| `pkg:xwayland` | `present: true` | yes — Debian Xwayland 24.1.13; `/bin/Xwayland`. Session Steam is a River X11 client (`DISPLAY=:0`). **`-glamor off`** on SI (river radeonsi SIGBUS’d on tiled BOs; mesa 26.2.1 libgallium needs GLIBC_2.43). Needs `xkbcomp` in the pack and `/usr/share/X11/xkb` → river’s xkeyboard-config. |
| `pkg:gamescope` | `present: false`, `requires.drm_modifiers` | yes — windowed nest compositor for Arcade Play (`--backend wayland`). Apply `present=true` is **refused** unless a connected GPU exports Vulkan WSI DRM format modifiers (AMD GFX6–8 and virtio-gpu do not). Not installed on canto (Pitcairn `1002:6810`). |
| `pkg:mesa` | `present: true` | yes — 64-bit OpenGL/GLX/EGL + Vulkan WSI (Debian mesa 26.2.1; `bash image/pack-mesa.sh`). `/bin/vulkaninfo`. ICD `share/vulkan/icd.d/radeon_icd.json` (64-bit) and `radeon_icd32.json` (32-bit Steam); live nodes `radeon_icd.x86_64.json` + `radeon_icd.i686.json`. LLVM 64-bit in `pkg:river`; 32-bit LLVM 21 + `libdisplay-info.so.3` + `libxml2.so.16` + `libwayland-client` 1.26 (`wl_fixes_interface`) in `pkg:mesa/lib32`. The ICD is DT_RPATH’d so Steam’s steamrt `LD_LIBRARY_PATH` (wayland 0.3.0, no `wl_fixes`) cannot hide the new client. |
| `pkg:steam` | `present: true` | yes — Valve steam-launcher + ubuntu12_32 bootstrap + 32-bit loader in `lib32` + 64-bit steamrt3 SONAMEs in `lib64`. `/bin/steam` (wrapper). User state is `~/.steam` and `~/.local/share/Steam`. The wrapper creates host nodes Steam’s scripts assume: `/usr/bin/env` + `/usr/bin/bash`, `/lib64/ld-linux-x86-64.so.2`, `/lib/ld-linux.so.2` + `/lib/i386-linux-gnu`, `/etc/ssl/certs/ca-certificates.crt`, `/bin/ldd`. It must **not** rewrite `pkg:glibc` `libresolv.so.2` (Ubuntu folded resolv into libc; this glibc still ships a separate libresolv that tmux NEEDs). srt-logger gets libresolv from `pkg:steam/lib/srt`. Busybox `xz`/`tar` lack `--robot` / `--blocking-factor`; shims live in `pkg:steam/libexec` and are prepended to PATH. 32-bit `libGL` (and gtk/pulse) come from the user’s steamrt3c i386 tree, SONAME-linked beside `steamui.so` — putting 64-bit `pkg:sola` `libGL` on `LD_LIBRARY_PATH` is `wrong ELF class: ELFCLASS64`. 64-bit helpers get `liboath-glclass.so` (LD_PRELOAD) so `gldriverquery` does not load the 32-bit `libGL`. Do not patchelf live `steamui.so`. Launcher **Steam** is `~/.config/sola/shell/applications.json` (`app_id=steam`, `/bin/steam`) and a sticky bus `Application` in `state.yaml`. `/bin/steam` is a normal X11 window on River `+xwayland` (`DISPLAY=:0`). steamwebhelper does **not** use pressure-vessel (`CLONE_NEWUSER` is EPERM after PID 1 chroot); `/bin/steam` sets `STEAM_RUNTIME_STEAMRT` to `pkg:steam/libexec/pv-host` and 64-bit GLX from `pkg:mesa`. Wrapper also ensures `hosts: files dns`, `127.0.0.1 steamloopback.host`, and `/bin/lsof` (`libexec/oath-lsof`; steamui runs `lsof -P -F upnR -i TCP@…` to identify the CEF websocket). Canto this boot: launcher **Steam**; session `/bin/steam` is River `+xwayland` `:0` (user confirmed). `STEAM_FRAME_FORCE_CLOSE=1` on `:0`; Super+Q SIGTERMs X11 class `steam`. 32-bit RADV inits (GpuTopology PITCAIRN); `liboath-peercred` is loaded into steamui via a `dlmopen` copy (`steamui.oath.so`) — do not patchelf live `steamui.so` (verifier re-extracts). steamwebhelper is `--disable-gpu` (CEF GPU SIGBUS on Pitcairn). The shim is first DT_NEEDED of a `dlmopen` copy and must `dlopen` `libSDL3`/`libX11` itself (`RTLD_NEXT` is NULL in that NS). It fakes `GAMESCOPE_VIEWPORT_SUPPORTED=0`. HDR atom stays 0. SDL display queries that return 0×0 are clamped to 1920×1080. `compose.dir` aliases C.UTF-8. |
| `pkg:hello` | `present: false` | yes — canary |
| `pkg:fetchme` | `present: false`, `url` | yes — wget canary |

`/bin/hello` prints `hello`. The symlink target is
`/oath/store/pkg/hello/<hash>/bin/hello`. Do not exec from the store; `/bin`
is how you run what is installed.

`present=false` on a non-removable package is **refused** (not
`--confirm`). PID 1 stays at `/lib/oath/init`; it is not a
package.

`requires.drm_modifiers` on a package means apply `present=true`
is refused unless a connected GPU exports Vulkan WSI DRM format
modifiers. That is how `pkg:gamescope` stays off canto. Uninstall
is always allowed.

Optional backup hooks in the store tree (T32/T33), not catalog
fields:

```
/oath/store/pkg/<name>/live/libexec/oath-backup-quiesce
/oath/store/pkg/<name>/live/libexec/oath-backup-thaw
```

`backup-send` runs quiesce on **present** packs that shipped the
executable, snapshots, then thaw. Missing hook: crash-consistent
only. Mention the hook in that pack’s `INDEX.md` if you ship one.

## Install / remove

```
oath ls --kind pkg
oath get pkg:hello
oath set pkg:hello present=true
oath apply
hello
readlink /bin/hello

oath set pkg:hello present=false
oath apply
oath undo
```

`present=false` removes **this object’s** `/bin` links. The store tree
stays (re-install needs no network). Apply refuses to clobber a `/bin`
name it does not own.

Takes effect on apply. Reboot is not required; it only proves the
symlink survived on `@`.

`pkg:pipewire` is the seat audio graph, not a Sola blob. Without it the
shell **hides** the volume chip. There is no udevd, so HDMI cards are
not auto-enumerated; canto pins Intel PCH analog as **Built-in Audio**
(`hw:0,0`) in the packed `pipewire.conf.d`. `wpctl status` as `home`
(`XDG_RUNTIME_DIR=/run/user/1`) is the check.

`hash` on a `pkg` object is the pin. Apply verifies the tree matches.
Empty hash is discovery (`actual.hash` is reported). Switch with
`oath set pkg:foo hash=sha256-…` then apply. Extra hashes stay on disk.

`url` on a `pkg` object: if `present` and the store is missing, apply
wget’s the URL then links. A `.tar` is a pack tree (hash is of the
tree). An origin prefix plus `hash` fetches
`{url}/pkg/{name}/{hash}.tar` (object storage is just HTTPS). A single
file (canary `pkg:fetchme`, `http://10.0.2.2:18765/fetchme` on QEMU
user net) becomes `bin/<name>`. Local store wins if the realization is
already there.

Host development cache (same layout): `.cache/oath/store` or
`$OATH_STORE`.

```
cargo make store --name hello --from apps/hello --tar
cargo make publish
```

`publish` copies `{hash}.tar` files into the origin tree (sibling
Wicket `extras/oath-store/site`, or `build/store-origin`). Public
bootstrap (live):
`https://store.oath.wicket.cloud/pkg/<name>/<hash>.tar`.
