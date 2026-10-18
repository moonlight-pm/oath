# Desk

The desk is the graphical session: rivulet (Oath's compositor, River
0.4.5 ported to C, R19) on wlroots, Mesa, libinput and Xwayland, all
built natively from `pkgs/`, with Sola on top. It is phase 8 of the
[roadmap](../roadmap.md); the Steam and Minecraft desk
([scope.md](scope.md)) builds on it.

**Status:** packages built on the build host 2026-10-18 and rivulet
runs in the build sandbox; not yet in a profile, not yet run on a
machine.

## Packages (built)

`cargo make pkgs rivulet` builds 52 packages in dependency order; 41 of
them are the desk's:

| Layer | Packages |
|-------|----------|
| Compositor | `rivulet` (`tree = "apps/rivulet"`), `wlroots` 0.20.2 with Oath's two patches (`pkgs/wlroots/patches/`, from `forks/wlroots`) |
| Wayland | `wayland`, `wayland-protocols`, `libffi`, `expat` |
| GPU | `mesa` 26.2.5 (radeonsi compiled by ACO, softpipe; GBM, EGL, GLES, GL and GLX; no LLVM, no Vulkan), `libdrm` (amdgpu, radeon), `elfutils`' libelf |
| Input and seat | `libinput` (no libwacom, mtdev or Lua), `libevdev`, `libudev-zero`, `seatd` (seatd, libseat with the seatd, builtin and noop backends) |
| Keyboard | `libxkbcommon`, `xkeyboard-config` |
| Display data | `libdisplay-info`, `hwdata` (pnp.ids only), `pixman` |
| X11 | `xwayland` 24.1.14 (glamor on GBM, GLX), `xkbcomp`, `libx11`, `libxcb`, `xcb-proto`, `libxau`, `xorgproto`, `xtrans`, `libxext`, `libxfixes`, `libxxf86vm`, `libxshmfence`, `libxkbfile`, `libxcvt`, `libxfont2`, `libfontenc`, `libepoxy`, `nettle` (SHA-1), `xcb-util-wm`, `xcb-util-errors` |
| Build tools | `python-mako`, `python-markupsafe`, `python-pyyaml`, `python-packaging` (Mesa's build) |

The 41 products are 65 MB; Mesa is 31 MB of it (radeonsi, swrast,
`libGL`, no LLVM). Rebuilding nettle rebuilt Xwayland, wlroots and
rivulet to the same hashes. rivulet's lock entry follows `apps/rivulet`:
after a change there, `cargo make pkgs rivulet` before an image build.

`apps/rivulet/test/sandbox.sh` runs the built rivulet in the build
sandbox, headless on the pixman renderer; its init command, `xkbcomp`
as an X client, starts Xwayland (glamor falls back to software without
a GPU), and reads the `us` keymap back. On 2026-10-18 it passed.

How they fit the sandbox:

- Data paths are other packs' slots, compiled in: libxkbcommon and
  xkbcomp read `/oath/store/pkg/xkeyboard-config/live/…`, Xwayland
  runs `/oath/store/pkg/xkbcomp/live/bin/xkbcomp`, wlroots runs
  Xwayland from its slot, Mesa and libinput read their own `share/`.
- The sandbox looks for `.pc` files only in `<pack>/lib/pkgconfig`, so
  data-only packs (wayland-protocols, xkeyboard-config, xorgproto,
  xtrans, xcb-proto, hwdata) move theirs there.
- The sandbox has no `/usr/bin/env`. A Python script meson would run by
  its `#!/usr/bin/env` line is made non-executable, so meson runs it
  with its own python (libevdev, libdisplay-info); xkeyboard-config's
  perl script gets perl's path in its `#!` line.
- Python modules are directories under `$LIVE/lib/python`; Mesa's
  build puts them on `PYTHONPATH`.
- A build pack brings its `run` packs, not what its `.pc` file
  requires. The X libraries' `.pc` files require xorgproto's, which is
  headers only and no one's `run`, so every package that builds against
  libxcb, libX11 or Mesa's `gl.pc` names `xorgproto` in `build`.
- Mesa has GLX, so there is a `libGL`: libepoxy aborts when Xwayland's
  glamor makes a desktop GL context and no `libGL` exists.

## Signatures to watch

Seven keys were added for the desk (Joshua, 2026-10-18). Facts that
will matter at the next version bump:

- `hutterer` is DSA-1024 and signs with SHA-1 (libevdev, Xwayland,
  xkbcomp, libXfont2). Xwayland's signer alternates: Olivier Fourdan
  (DSA-1024 `67DC86F2623FC5FD4BB5225D14706DBE1E4B4540`, from
  `gitlab.freedesktop.org/ofourdan.gpg`) signed 24.1.10, 24.1.11 and
  the 26.x candidates.
- `coopersmith` holds two keys; the 2024 one (`3AB2…E2D2`) expires
  2026-10-24 and the older one's signing subkey 2028-04-19. The
  verifier ignores expiry, but a refreshed export is due.
- `turner` holds only Matt Turner's current Ed25519 signing subkey: the
  verifier checks no revocations, and his revoked subkeys stay out.
- Mesa 26.3 is signed by Dylan Baker (Ed25519
  `71C4B75620BC75708B4BDB254C95FAAB3EB073EC`), not `engestrom`. Mesa's
  published keyring holds a copy that expired 2020; keyserver.ubuntu.com
  has it extended.
- `expat` expires 2026-12-28; `ser` 2027-04-12. libdrm's and
  wayland-protocols' release managers rotate (Samuel Pitoiset, Jonas
  Ådahl signed earlier releases).
- `gmp` (Niels Möller) had expired before it signed nettle 4.0.

Twelve sources have no signature the verifier can check, each saying
why in its `pkg.toml`: libffi, libudev-zero, libxkbcommon, seatd,
libepoxy, mako and PyYAML publish none; libinput (since 1.19.4)
and hwdata sign only git tags; pixman signs only an inline-signed sha512
list; MarkupSafe and packaging have PEP 740 attestations (Sigstore DSSE),
which `sigstore.rs` does not read. Teaching the verifier those two
formats would sign three of them. Forge tag archives (libinput,
libudev-zero, libxkbcommon, hwdata) can be regenerated with new bytes;
a sha256 mismatch on one of those is not by itself an attack.

## Known defects

- Nothing has run on a machine yet: the DRM and libinput backends, the
  seat, radeonsi on canto's Pitcairn GPUs and glamor are untested.
- Xwayland admits local clients by user name (`si:localuser`), so the
  desk user needs an `/etc/passwd` entry, and wlroots' shm buffers need
  `/dev/shm`. A machine has both (PID 1 mounts `/dev/shm`); the build
  sandbox has neither, so `sandbox.sh` makes them.
- libudev-zero listens for hotplug on netlink group 4, not the
  kernel's group 1, so libinput sees only the devices present at start
  until something rebroadcasts uevents. It always reports devices as
  initialized and lacks udev's rule-made properties (`ID_INTEGRATION`,
  `LIBINPUT_DEVICE_GROUP`, calibration), so libinput falls back to its
  quirks.
- rivulet keeps three upstream River bugs (see
  [apps/rivulet/README.md](../../apps/rivulet/README.md)).

## Target

- **The seat.** Who opens `/dev/dri` and `/dev/input`: seatd as a
  service, or libseat's noop backend with PID 1 granting the desk user
  the nodes. PID 1 rebroadcasts uevents for libudev-zero.
- **Services.** `svc:rivulet` and the Sola session in a desk profile
  that owns the screen (R15).
- **Sola natively.** oath-sola starts and reaps `rivulet`, not `river`
  (`forks/sola/crates/sola/src/river.rs`); then audio, fonts and Sola's
  own packages.
- **Later:** Vulkan (RADV), 32-bit Mesa and glibc for Steam.
