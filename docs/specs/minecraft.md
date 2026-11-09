# Minecraft

Minecraft Java Edition on the desk (R74): Prism Launcher built from source
on Qt 6, Java from Eclipse Temurin's signed release, in a bubblewrap box, on
a root with no `/lib64` or `/usr`. It is the Minecraft half of phase 9
([roadmap](../roadmap.md), [scope.md](scope.md)); [steam.md](steam.md) is
the other.

**Status:** is, on koeurax since 2026-11-08 (generation 5, each package its
own head on the origin, in Sola's launcher as Minecraft). Joshua signed in and
ran it that day: Minecraft 1.21.5 on Fabric Loader 0.16.14 with the restored
mods, LWJGL 3.3.3, OpenGL on the RTX 4060, sound on the onboard HDA. Prism plays
nothing, offline or not, until a Microsoft account that owns the game is added.

## Why Prism

| Launcher | On Linux | Why not |
|----------|----------|---------|
| Modrinth App | Tauri: WebKitGTK and GTK3, a 120 MB AppImage of Ubuntu's | Oath ships neither; blank screens and crashes on NVIDIA (modrinth/code#3057, open since 2024-12); PostHog analytics; its Java download is unchecked. Its Windows build under Proton needs WebView2, which Wine cannot draw. |
| Prism's own builds | sharun bundles of Ubuntu 24.04's Qt (and loader) | another distribution's libraries (R7), unsigned files |
| Our own launcher | | Microsoft's sign-in for Minecraft needs an application ID that Mojang reviews; mod loaders' installers |
| PortableMC | a signed Rust CLI | no window to manage instances and mods |

Prism installs mods and modpacks from Modrinth and CurseForge itself, so
Modrinth's catalog stays.

## What the game assumes

| Assumes | Here |
|---------|------|
| a launcher that downloads Java (`/lib64/ld-linux-x86-64.so.2`) | Prism's Java downloader is off (its default on Linux); every `pkg:jre-*` on the machine is offered through `PRISMLAUNCHER_JAVA_PATHS` |
| Java 21 for 1.20.5–1.21.11, 17 for 1.18–1.20.4, 25 for 26.1 on | `pkg:jre-21`; the others will be `pkg:jre-17` and `pkg:jre-25`, each in its own `libexec/jre-<n>` |
| its native libraries (LWJGL, GLFW or SDL, OpenAL), downloaded per version | shared objects, with no loader of their own: they find `libGL`, `libX11`, `libXcursor`, `libXi`, `libXrandr`, `libpulse`, `libpipewire`, `libasound` and `libvulkan` in `/lib`, which `pkg:prismlauncher`'s `run` brings |
| X11 (GLFW, to 26.2) | Xwayland `:0`; from 26.3 SDL3 prefers Wayland when the compositor offers `wp_fifo_manager_v1` |
| OpenGL; Vulkan 1.2 (26.2's option, 26.4's default) | libglvnd to Mesa or NVIDIA; `pkg:vulkan-loader` |
| `/usr/lib/jvm` | not looked in |

## Packages

| Package | What |
|---------|------|
| `prismlauncher` | 11.1.1 from its tag, signed by Seth Flynn (`flynn`), with `libnbtplusplus` at the commit the tag names. `libexec/prismlauncher/` (the program and its two jars), `bin/prismlauncher` (the box), the desktop entry and icons in `share/`. No updater, no Java downloader; `gamemode_client.h` is a stub that says GameMode is absent. |
| `jre-21` | Temurin 21.0.13+9, Adoptium's JRE, checked against its signature (`adoptium`). Its programs are pointed at `/lib`'s loader; `lib/libjvm.so` names `lib/server/`'s for the loads check. |
| `jdk-21` | Temurin's JDK, a build pack (Prism's Java parts). |
| `qtbase`, `qtsvg`, `qtimageformats`, `qtnetworkauth` | Qt 6.11.3. qtbase on Wayland alone (no xcb), EGL and desktop GL, OpenSSL linked, no D-Bus, GLib, ICU, SQL, printing or Android and WebAssembly tools; not relocatable; its TLS reads `pkg:certs`; its CMake files, and its modules', are in `share/cmake` (a pack's `lib/` is linked into `/lib`, which has one `lib/cmake`). It bundles PCRE2 (16-bit), libpng, libjpeg, double-conversion, md4c, libb2, libpsl. qtimageformats ships the WebP plugin alone (Modrinth's icons), with its own libwebp. Qt publishes md5 sums, no signatures: each tarball is set against its tag. |
| `cmark`, `libarchive`, `libqrencode` | Mod descriptions, archives (zlib, bzip2, xz, zstd; no programs), the sign-in QR Code. libarchive is signed by Martin Matuška (`matuska`). |
| `extra-cmake-modules`, `tomlplusplus` | Build packs. ECM is signed by Nicolas Fella (`fella`, KDE's release keyring). |
| `libxi`, `libxtst`, `libxcursor` | The JRE's AWT and the game's GLFW or SDL. |

## The box (`bin/prismlauncher`)

A mod is Java code from the internet, and it runs as whoever plays; `home`
has `sudo` with no password (R42). So `bin/prismlauncher` starts Prism, and
every game it launches, in `bwrap`:

- the root read-only; `/home`, `/run` and `/tmp` empty tmpfs;
- read-write: `~/.local/share/PrismLauncher` (instances, worlds, mods,
  screenshots, the accounts file) and `~/.cache/PrismLauncher` as
  `~/.cache` (shader caches);
- the Wayland socket, Xwayland's socket, PipeWire's and pipewire-pulse's;
- `/dev` new, with `/dev/dri`, NVIDIA's nodes and `/dev/input` bound in
  (only pads are `home`'s there, R44); not `/dev/uinput`, which could type
  into the desk;
- its own PID namespace; no D-Bus; `no_new_privs`, so `sudo` cannot run.

It keeps `home`, not `work` (R55): the game needs the desk's display, sound
and pads, and opening them to `work` would open them to every server on a
machine that has some.

**Gaps (is):** the packs and heads are on the origin, but the plans and source packs that R13 puts beside them go up with the next release. The game's narrator logs an error at start (no `libflite`) and stays off. The box shares the machine's network, so it reaches
localhost services (Steam's ports on a desk) and abstract sockets
(Xwayland's); through Xwayland it sees other X11 clients' input (Steam).
Prism's "open folder" and links do nothing (no `xdg-open`); sign-in uses
the code shown with its QR Code. On a fresh machine, Prism's first session
still defaults `AutomaticJavaDownload` on when no Java is set, though its
downloader is compiled out of the interface, and a Java it fetched would name
`/lib64`: koeurax's `prismlauncher.cfg` sets `JavaPath`, `AutomaticJavaSwitch`
and `AutomaticJavaDownload=false`.

## Bringing a Modrinth App profile over

A Modrinth App profile is a game directory
(`%APPDATA%\ModrinthApp\profiles\<name>` on Windows,
`~/.local/share/ModrinthApp/profiles/<name>` on Linux); its version and
loader are in the app's database, `app.db`, and the game's own
`logs/latest.log` says them too (`Loading Minecraft 1.21.5 with Fabric
Loader 0.16.14`). A Prism instance with that version and loader takes the
directory's `mods`, `config`, `options.txt`, `resourcepacks`,
`shaderpacks`, `saves`, `screenshots` and `servers.dat` in its
`minecraft/`. A world opened in a newer version is upgraded for good;
keep the copy.

## Target

- **Will be:** `pkg:jre-17` and `pkg:jre-25`.
- **Will be:** a pad tested in a game (Controlify, through `/dev/input`).
- **Is, per machine:** an entry in the desk's launcher. Sola's list is its own
  (`Application:` in `state.yaml`, set from Settings → Applications) and reads
  no `share/applications`: command `/bin/prismlauncher`, its icon in the pack's
  `share/icons/hicolor/256x256/apps/`.
