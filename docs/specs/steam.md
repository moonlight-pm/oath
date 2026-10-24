# Steam

Steam on the desk (R29): Valve's launcher as a recipe, run in a sandbox
that gives Steam the root it assumes, on a machine whose root stays
Oath's. It is the Steam half of phase 9 ([roadmap](../roadmap.md),
[scope.md](scope.md)); it builds on the [desk](desk.md).

**Status:** is, on canto since 2026-10-23: `pkg:steam`, built there
from its recipe, with `pkg:bubblewrap`, `pkg:libxcrypt` and the i386
packages (`pkg:gcc` with `-m32`, glibc, Mesa and 15 libraries). Valve's
64-bit client installs and updates itself in the sandbox and is signed
in; its webhelper renders on the GPU (ANGLE on radeonsi, Mesa 26.2.5);
its container imports both Mesas, and its 32-bit GL and Vulkan probes
find radeonsi and RADV. Will be: a 32-bit game tested, controllers (a
reboot for the kernel's pad drivers, and node rules), a launcher entry.

## What Steam assumes

Valve's
[distro-assumptions.md](https://gitlab.steamos.cloud/steamrt/steam-runtime-tools/-/blob/main/docs/distro-assumptions.md)
lists it. Against a machine's root:

| Steam assumes | Oath's root | In Steam's sandbox |
|---------------|-------------|--------------------|
| `/lib64/ld-linux-x86-64.so.2` | `/lib/ld-linux-x86-64.so.2` only | `/lib64` → `lib` |
| `/lib/ld-linux.so.2` and i386 libraries (glibc, libGL, …) | none | from every pack's `lib32/` |
| `/usr/bin/env`, `/usr/lib`, `/usr/share` | no `/usr` | a merged `/usr`: the root's links in `/usr/bin`, `/usr/sbin`, `/usr/lib`; `/usr/share`, every pack's `share/` |
| `/sbin/ldconfig -XNv`, `/etc/ld.so.cache` | neither | glibc's `ldconfig`; the cache made on first start |
| CA bundle at `/etc/ssl/certs/ca-certificates.crt` | `SSL_CERT_FILE` only | `pkg:curl`'s bundle at the usual three paths |
| `libcrypt.so.1` | glibc 2.44 has none | `pkg:libxcrypt` |
| unprivileged user and mount namespaces | yes | Steam's containers nest; its probe runs `true` in a root of `/usr`, `/bin`, `/lib*`, `/sbin` and `/etc` alone, so a static one is at `/usr/local/bin/true` |
| libraries' real paths under `/usr`, `/lib*`, `/nix` or `/gnu/store` | under `/oath/store` | `PRESSURE_VESSEL_FILESYSTEMS_RO=/oath/store` |
| X11, PulseAudio, a D-Bus session | Xwayland `:0`, `pipewire-pulse`, `svc:dbus-session` | the same |

## Packages

| Package | What |
|---------|------|
| `steam` | Recipe (R23) of `steam_1.0.0.87.tar.gz` from repo.steampowered.com, sha256-pinned, its `.dsc` clearsigned by Valve (`pkgs/keys/valve.asc`, `DB23C56B…879D59CB`). `libexec/steam/` holds Valve's `bin_steam.sh` and bootstrap tarball; `bin/steam` is the sandbox; `libexec/steam/bin/ldd` knows both loaders (steam.sh's library check). `libexec/steam/true` is a 129-byte static ELF (exit 0), written by `printf`. Runs with `bubblewrap`, `glibc-i386`, `libxcrypt`, `libxcrypt-i386` and `mesa-i386`. |
| `glibc-i386` | Built. glibc 2.44, i386: `lib32/` with `ld-linux.so.2`, slibdir `/lib32` (`${LIB}` is `lib32`, Arch's pair), locales from `/lib/locale`. Built by a C compiler made in the same plan from `pkg:gcc`'s tarball for its 32-bit libgcc. Headers are `pkg:glibc`'s but `gnu/stubs-32.h` and `gnu/lib-names-32.h`. |
| `libxcrypt` | Built. `libcrypt.so.1`, glibc's crypt ABI. |
| `bubblewrap` | Built. `bwrap` 0.13.1, unprivileged (not setuid). |
| `<name>-i386` | Built. zlib, zstd, expat, libffi, wayland, libdrm, elfutils (libelf), libxau, libxcb, libx11, libxext, libxfixes, libxxf86vm, libxshmfence, libxcrypt and mesa (radeonsi, RADV, softpipe, GLX, EGL, GBM). |

32-bit packs are `<name>-i386` with a `lib32/`. A machine's `/lib` farm
links only `lib/`, so nothing 32-bit is in a machine's root; a build
sandbox (every pack's `lib32/` at `/lib32`, the loader at
`/lib/ld-linux.so.2`) and Steam's sandbox show them.
`pkgs/<name>-i386/pkg.toml` with `i386 = "<name>"` builds that package's
sources and build.sh for i386: its own `build` and `run` name i386 packs,
pkg:glibc-i386 comes with it, and the build sees `LIBDIR=lib32` and
`cc -m32`, with each pack's `lib32/pkgconfig` before any `lib/pkgconfig`.
A build.sh that serves both uses `$LIBDIR` and ships `bin/` only for
`lib`, since a pack's `bin/` is linked into `/bin`.

## The sandbox (`bin/steam`)

`bin/steam` builds a tree under `$XDG_RUNTIME_DIR/oath-steam/<key>`,
keyed by the script and every pack's live realization, and starts
Valve's `bin_steam.sh` in `bwrap` with a merged-`/usr` root:

- `/usr/bin`, `/usr/sbin`, `/usr/lib`: real directories holding the
  root's `/bin`, `/sbin` and `/lib` links (absolute, into the store),
  glibc's `sbin/` (`ldconfig` is static-pie) and `ld-linux.so.2` from
  the packs' `lib32/`; `/lib`'s real directories (`firmware`,
  `modules`, `oath`) bound in. They are real because Steam's containers
  mount this `/usr` at `/run/host/usr`, where a link to `/sbin` would
  name the container's.
- `/usr/lib32` (every pack's `lib32/`), `/usr/share` (every pack's
  `share/`, two levels deep), `/usr/local/bin/true`;
- `/bin`, `/sbin`, `/lib`, `/lib32`, `/lib64` → `usr/…`;
- `/etc` from the tree (`ld.so.conf`, the cache, the CA bundle), the
  root's entries bound over their names;
- the root's other directories as they are (`/oath`, `/boot`
  read-only; `/home`, `/run`, `/tmp`, `/var` read-write; `/dev`,
  `/proc` and `/sys` bound, not new), the host's PID and network
  namespaces.

Old trees stay under `$XDG_RUNTIME_DIR` (a tmpfs) until the next boot.

On a first run (no `~/.local/share/Steam`) it opts into Valve's 64-bit
client: `package/beta` is `publicbeta` and `.steam-enable-steamrt64-client`
exists. Steam's settings change them afterwards.

## Target

- **Will be:** a 32-bit game run (Valve suggests Floating Point:
  small, 32-bit, OpenGL).
- **Will be:** controllers. The kernel fragment adds `uinput`, `joydev`,
  `xpad`, `hid-steam`, `hid-nintendo`, `hid-microsoft` and
  `hid-playstation` (`pkg:linux` 333089ed, on canto's ESP, live after a reboot);
  `/dev/input/event*`, `/dev/uinput` and `/dev/hidraw*` are root's
  (0600), so the seat's user needs node rules (devices.md).
- **Will be:** a check that i386 ELFs load what their run packs have
  (`check_loads` reads x86_64 only).
- **Will be:** a launcher entry on the desk.
- **Upstream:** asking Valve to add `/oath/store` next to `/nix` and
  `/gnu/store` in pressure-vessel's framework paths (Joshua's call).
