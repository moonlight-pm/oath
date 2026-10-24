# Steam

Steam on the desk (R29): Valve's launcher as a recipe, run in a sandbox
that gives Steam the root it assumes, on a machine whose root stays
Oath's. It is the Steam half of phase 9 ([roadmap](../roadmap.md),
[scope.md](scope.md)); it builds on the [desk](desk.md).

**Status:** being brought up on canto (2026-10-23). `pkg:glibc32`,
`pkg:libxcrypt`, `pkg:bubblewrap` and the `pkg:steam` recipe are built;
the client installs and updates itself in the sandbox. Not yet:
32-bit Mesa, controllers, a launcher entry.

## What Steam assumes

Valve's
[distro-assumptions.md](https://gitlab.steamos.cloud/steamrt/steam-runtime-tools/-/blob/main/docs/distro-assumptions.md)
lists it. Against a machine's root:

| Steam assumes | Oath's root | In Steam's sandbox |
|---------------|-------------|--------------------|
| `/lib64/ld-linux-x86-64.so.2` | `/lib/ld-linux-x86-64.so.2` only | `/lib64` → `lib` |
| `/lib/ld-linux.so.2` and i386 libraries (glibc, libGL, …) | none | from every pack's `lib32/` |
| `/usr/bin/env`, `/usr/lib`, `/usr/share` | no `/usr` | `/usr/{bin,sbin,lib,lib32,lib64}` → the root's; `/usr/share`, every pack's `share/` |
| `/sbin/ldconfig -XNv`, `/etc/ld.so.cache` | neither | glibc's `ldconfig`; the cache made on first start |
| CA bundle at `/etc/ssl/certs/ca-certificates.crt` | `SSL_CERT_FILE` only | `pkg:curl`'s bundle at the usual three paths |
| `libcrypt.so.1` | glibc 2.44 has none | `pkg:libxcrypt` |
| unprivileged user and mount namespaces | yes | Steam's containers nest |
| libraries' real paths under `/usr`, `/lib*`, `/nix` or `/gnu/store` | under `/oath/store` | `PRESSURE_VESSEL_FILESYSTEMS_RO=/oath/store` |
| X11, PulseAudio, a D-Bus session | Xwayland `:0`, `pipewire-pulse`, `svc:dbus-session` | the same |

## Packages (built)

| Package | What |
|---------|------|
| `steam` | Recipe (R23) of `steam_1.0.0.87.tar.gz` from repo.steampowered.com, sha256-pinned, its `.dsc` clearsigned by Valve (`pkgs/keys/valve.asc`, `DB23C56B…879D59CB`). `libexec/steam/` holds Valve's `bin_steam.sh` and bootstrap tarball; `bin/steam` is the sandbox; `libexec/steam/bin/ldd` knows both loaders (steam.sh's library check). Runs with `bubblewrap`, `glibc32`, `libxcrypt`. |
| `glibc32` | glibc 2.44, i386: `lib32/` with `ld-linux.so.2`, slibdir `/lib32` (`${LIB}` is `lib32`, Arch's pair), locales from `/lib/locale`. Built by a C compiler made in the same plan from `pkg:gcc`'s tarball for its 32-bit libgcc. Headers are `pkg:glibc`'s but `gnu/stubs-32.h` and `gnu/lib-names-32.h`. |
| `libxcrypt` | `libcrypt.so.1`, glibc's crypt ABI. |
| `bubblewrap` | `bwrap` 0.13.1, unprivileged (not setuid). |

32-bit packs are `<name>32` with a `lib32/`. A machine's `/lib` farm
links only `lib/`, so nothing 32-bit is in a machine's root; a build
sandbox and Steam's sandbox show `lib32/` (is: Steam's; will be: the
build sandbox's, for the 32-bit compiler).

## The sandbox (`bin/steam`)

`bin/steam` builds a tree under `$XDG_RUNTIME_DIR/oath-steam/<key>`,
keyed by the script and every pack's live realization, and starts
Valve's `bin_steam.sh` in `bwrap` with:

- the root's directories as they are (`/bin`, `/oath`, `/boot`
  read-only; `/home`, `/run`, `/tmp`, `/var` read-write; `/dev`,
  `/proc` and `/sys` bound, not new), the host's PID and network
  namespaces;
- `/lib` the root's links plus `ld-linux.so.2`, and its real
  directories (`firmware`, `modules`, `oath`) bound in;
- `/lib32`, `/lib64`, `/usr`, `/sbin` and the extra `/etc` entries from
  the tree; the root's `/etc` entries bound over their names.

On a first run (no `~/.local/share/Steam`) it opts into Valve's 64-bit
client: `package/beta` is `publicbeta` and `.steam-enable-steamrt64-client`
exists. Steam's settings change them afterwards.

## Target

- **Will be:** a 32-bit-capable `pkg:gcc` (`--enable-multilib`, `m32`
  in `lib32/`), `libxcrypt32`, and 32-bit Mesa (radeonsi, RADV) with
  its X11, drm, wayland, expat, zlib, zstd and libelf packs, for 32-bit
  games. That rebuilds every plan once.
- **Will be:** controllers. The kernel lacks `uinput`, `joydev`, `xpad`,
  `hid-steam`, `hid-nintendo`, `hid-microsoft` and `hid-playstation`;
  `/dev/uinput` and `hidraw` need the seat's permissions.
- **Will be:** a launcher entry on the desk.
- **Upstream:** asking Valve to add `/oath/store` next to `/nix` and
  `/gnu/store` in pressure-vessel's framework paths (Joshua's call).
