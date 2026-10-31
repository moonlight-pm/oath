# Desk

The desk is the graphical session: rivulet (Oath's compositor, River
0.4.5 ported to C, R19) on wlroots, Mesa, libinput and Xwayland, all
built natively from `pkgs/`, with Sola on top. It is phase 8 of the
[roadmap](../roadmap.md); the Steam and Minecraft desk
([scope.md](scope.md)) builds on it.

**Status:** built and running on canto since 2026-10-18 (the `desk`
profile, R20–R22): rivulet drives DP-10 with radeonsi on ACO, input
comes through seatd, Xwayland serves X clients, and Sola's session runs
with its terminal, Projects and browser, sound through PipeWire, Noto fonts, the
tools that build Sola on the machine, and novus's keyboard and mouse
through `sola-kvm`.

## Packages (built)

The desk's packages, built in dependency order by `cargo make pkgs`
(`cargo make pkgs rivulet` builds the compositor and what it needs):

| Layer | Packages |
|-------|----------|
| Compositor | `rivulet` (`tree = "apps/rivulet"`), `wlroots` 0.20.2 with Oath's two patches (`pkgs/wlroots/patches/`, from `forks/wlroots`) |
| Wayland | `wayland`, `wayland-protocols`, `libffi`, `expat`, `wl-clipboard` 2.3.0 (`wl-copy`, `wl-paste`: the clipboard from a shell over rivulet's ext-data-control, which Claude Code pastes images with) |
| GPU | `mesa` 26.2.5 (radeonsi and RADV, compiled by ACO; softpipe; GBM, and EGL, GLES, GL and GLX as libglvnd's vendor, and Vulkan, with the device-select layer; no LLVM), `libglvnd` 1.7.0 (libEGL, libGL, libGLX, libOpenGL, libGLESv2; and `-i386`), `nvidia` 595.104.02 (a recipe, R34), `vulkan-loader`, `vulkan-tools` (`vulkaninfo`), `libdrm` (amdgpu, radeon), `elfutils`' libelf |
| Input and seat | `libinput` (no libwacom, mtdev or Lua), `libevdev`, `libudev-zero`, `seatd` (seatd, libseat with the seatd, builtin and noop backends) |
| Keyboard | `libxkbcommon`, `xkeyboard-config` |
| Display data | `libdisplay-info`, `hwdata` (pnp.ids only), `pixman` |
| X11 | `xwayland` 24.1.14 (glamor on GBM, GLX), `xkbcomp`, `libx11`, `libxcb`, `xcb-proto`, `libxau`, `xorgproto`, `xtrans`, `libxext`, `libxfixes`, `libxxf86vm`, `libxshmfence`, `libxkbfile`, `libxcvt`, `libxfont2`, `libfontenc`, `libepoxy`, `nettle` (SHA-1), `xcb-util-wm`, `xcb-util-errors` |
| Build tools | `python-mako`, `python-markupsafe`, `python-pyyaml`, `python-packaging`, `glslang` (Mesa's build: RADV's BVH shaders), `vulkan-headers` (the loader's and vulkaninfo's), `cmake` |

Mesa is 50 MB (radeonsi, RADV, swrast, `libGL`; no LLVM). Rebuilding
nettle rebuilt Xwayland, wlroots and rivulet to the same hashes; adding
RADV to Mesa rebuilt those, libepoxy and CEF, also to the same hashes.
rivulet's lock entry follows `apps/rivulet`: after a change there,
`cargo make pkgs rivulet` before an image build.

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
  libxcb, libX11 or libglvnd's `gl.pc` names `xorgproto` in `build`.
- libglvnd has GLX, so there is a `libGL`: libepoxy aborts when
  Xwayland's glamor makes a desktop GL context and no `libGL` exists.
  What builds against EGL or GL names `libglvnd` (its headers and
  `egl.pc`, `gl.pc`, `glesv2.pc`); `gbm.pc` is still Mesa's.

## Seat and profile (built)

The `desk` seed profile (R20) extends `base`:

- `svc:seatd` (root, `seatd -u home -g home`) opens the KMS device
  (`/dev/dri/card*`) and the input devices for whoever holds the seat,
  and takes them back on a VT switch.
- `svc:rivulet` runs `rivulet -log-level info -c :` as `home`, wanting
  `svc:seatd`, with `LIBSEAT_BACKEND=seatd`. Its log is
  `oath logs svc:rivulet`.
- The seat has two turns (R42, [login.md](login.md)): rivulet and Sola's
  seat services (`sola-river`, `sola-shell`, `sola-session`,
  `sola-kvm`) are the desk's, and `svc:screen` the console's, held
  while the desk has tty1. Exit Sola (or `oath desk stop`) hands tty1 to
  the console; `oath desk` hands it back. `svc:login` keeps the
  password Sola's lock screen asks for.
- rivulet's 35 packages, `pkg:gcc` among them (Mesa's libstdc++).

PID 1 does what udevd would for the seat: render nodes are mode 0666
(GL clients and wlroots open them directly) and every kernel uevent is
passed on to netlink group 4 for libudev-zero ([devices.md](devices.md)).
Services start before the deferred display modules load; wlroots waits
up to 10 s for a KMS device to appear there, and `restart: always`
covers a GPU that takes longer.

When wlroots cannot make a GPU renderer (QEMU's virtio-gpu has no Mesa
driver here), rivulet falls back to pixman instead of exiting, so the
desk image runs in QEMU too. The `desk` check case (`profile = "desk"`,
metal) asserts seatd and rivulet run, rivulet holds a `card` and
`event` nodes that seatd opened, a connector is connected, render nodes
are 0666, and an X client gets a keymap through Xwayland.

## Sola (built)

Sola packages itself (R28): its own repo builds `pkg:sola`,
`sola-browser` and `sola-design` on an Oath machine (`oath store` for a
dev pack, `oath build` of a plan it generates for a release) and
publishes them to the default origin. Oath builds none of them: the
`desk` profile pins each by `hash` and `needs` (Sola's release of
2026-10-29, from its `next` at `83df4558`, built against `pkg:cef` 156
with the `cef` crate 156.2.0; its engine asks libcef for API version
15600, not the experimental one), `cargo make build` fetches
them into the image, `publish` leaves them out, and `update` moves a
machine still at the pins its release set and keeps one its owner moved
(canto runs Sola's dev packs).

| Package | What |
|---------|------|
| `sola`, `sola-browser`, `sola-design` | Sola's packs, pinned (R28): the `/bin/sola-*` programs the services run, `solactl`, and `share/cursors` (McMojave) for `XCURSOR_PATH`. The browser and Design run on `pkg:cef`; sharing a canvas needs `cloudflared`, which Oath does not ship |
| `cef` | CEF 156.0.14 built from Chromium's and CEF's source with H.264 and AAC (R40), laid out as the vendor's minimal distribution: `cef/Release` (libcef.so, SwiftShader; not Chromium's Vulkan loader), `cef/Resources`; 609 MB. Sola's packs are built against it (R37) |
| CEF's libraries | `glib` (with `pcre2`), `nspr`, `nss`, `dbus`, `at-spi2-core` (patched: libxml2 only for its tests), `cups` (libcups only, no TLS; patched to build without DNS-SD), `libxcomposite`, `libxdamage`, `libxrandr`, `libxrender`, `cairo`, `pango`, `harfbuzz`, `fribidi`, `alsa-lib` |
| Text | `fontconfig` (its fonts.conf lists the font packages; `50-oath-families.conf` maps sans-serif, serif and system-ui to Noto, monospace to Iosevka Term Slab then Noto Sans Mono, emoji and math to Noto's; `45-noto-cjk.conf`, ahead of it, picks the CJK face by language; both bind weakly, so a page's language outranks the family list), `freetype`, `font-noto` (Sans, Serif, Sans Mono, Symbols, Math and the scripts web pages need, slim variable TTFs from the 2026.10.01 monthly tag), `font-noto-cjk` (Sans and Serif CJK, Regular and Bold OTCs), `font-noto-emoji` (Noto Color Emoji, CBDT), `font-iosevka-term-slab` (Regular and Bold), `locales` (C.UTF-8 and en_US.UTF-8 in `/lib/locale`) |
| Sound | `pipewire` 1.6.9 (`pipewire`, `pipewire-pulse`, the `pw-*` tools; no session manager of its own), `wireplumber` 0.5.18 (with Lua 5.5 built in; `50-oath.conf` turns off Bluetooth, portals and D-Bus device reservation), `libpulse` (PulseAudio 17's client, `autospawn = no`), `libsndfile` (PCM formats only), `libudev-zero` with upstream PR 69 so PipeWire's ALSA monitor sees sound cards; `svc:pipewire` and `svc:pipewire-pulse` have `rtprio` 95, so module-rt runs their data threads `SCHED_FIFO` without RTKit |
| Development | `rust`, `gcc`, `pkgconf`, `cmake` 4.4.4, `meson`, `ninja`, `make`, `perl`, `python`, `git` 2.56.0 (https through libcurl, which `curl` now ships with its headers) |
| Terminal | `tmux`, `libevent` |

PID 1 runs Sola's processes as `home` services (`svc:sola-bus`,
`sola-call`, `sola-river` wanting `svc:rivulet`, `sola-shell`,
`sola-session`); `crates/sola`, Sola's own process manager, does not
run. Their environment names what NixOS gets from its
session: `XCURSOR_PATH`/`XCURSOR_THEME` (rivulet too),
`FONTCONFIG_FILE` (iced parses fonts.conf itself), `SHELL=/bin/zsh`
(the login shell, R27). Sola logs to `/home/.local/state/sola`, its
default (`/oath/log` is root's). `host:local` sets
`LANG=en_US.UTF-8` (tmux refuses a non-UTF-8 locale) and
`SSL_CERT_FILE` (curl's Mozilla bundle, for rustls in the browser's
vault and for cargo), `DBUS_SESSION_BUS_ADDRESS` and `PKG_CONFIG_PATH`
(every desk package's `lib/pkgconfig`: `.pc` files stay with their
pack, so a build on the machine looks in each). Apps open with
`solactl session launch sola-terminal`, `sola-projects`,
`sola-browser` or `sola-design`, or from the shell's launcher.

**Vulkan.** Sola's apps draw with wgpu, which prefers Vulkan to GL:
RADV, Mesa's Vulkan driver for amdgpu (Vulkan 1.3 on GFX6).
`WGPU_BACKEND=gl` forces Mesa's EGL instead; CEF's pages draw in its
own GPU process, not through wgpu. `pkg:vulkan-loader`
(`/lib/libvulkan.so.1`, which wgpu opens with dlopen) finds drivers and
layers by their JSON manifests. There is no `/usr/share`, so the data
directories it falls back to when `XDG_DATA_DIRS` is unset are Mesa's
slot and NVIDIA's, compiled in as fontconfig names the font packs:
`/oath/store/pkg/mesa/live/share/vulkan/icd.d` (RADV) and
`implicit_layer.d` (device-select), then `pkg:nvidia`'s `share/vulkan`.
A program that sets `XDG_DATA_DIRS` has to list them.
`/etc/vulkan` and `~/.local/share/vulkan` are read too, and
`VK_DRIVER_FILES` replaces the search. The manifests name each library
by name, which glibc finds in `/lib`. There is one loader: `pkg:cef`
leaves out the `libvulkan.so.1` Chromium ships in `Release`, whose
search (`/etc/xdg`, `/usr/share`) finds nothing here, and which a
program with `Release` in its RUNPATH (Sola's browser and Design) would
load before `/lib`'s. CEF's SwiftShader (`vk_swiftshader_icd.json`,
named by `VK_ICD_FILENAMES`) runs through Oath's loader. With two GPUs, Mesa's
device-select layer lists the compositor's primary GPU first: it asks
the compositor over Wayland (linux-dmabuf feedback), else takes the
boot VGA card. `DRI_PRIME=pci-0000_02_00_0` puts another card first,
and `NODEVICE_SELECT=1` turns the layer off. `vulkaninfo --summary`
lists the devices, and `VK_LOADER_DEBUG=driver,layer` shows where the
loader looked.

**GL, EGL and NVIDIA (R34).** `pkg:libglvnd` owns `libEGL.so.1`,
`libGL.so.1`, `libGLX.so.0`, `libOpenGL.so.0` and `libGLESv2.so.2` and
sends each display's calls to its vendor: Mesa's `libEGL_mesa` and
`libGLX_mesa`, or NVIDIA's. EGL vendors are JSON files in
`/etc/glvnd/egl_vendor.d`, then `pkg:nvidia`'s slot, then Mesa's
(`share/glvnd/egl_vendor.d`, compiled in), so a machine with the card
tries NVIDIA's first; a GLX client gets the vendor the X server names
(Xwayland names NVIDIA's on its GBM). Mesa's libgbm loads a GPU's own
backend from its `lib/gbm` or, on x86_64, `pkg:nvidia`'s
`lib/nvidia/gbm` (`nvidia-drm_gbm.so`), so wlroots and Xwayland's
glamor allocate on the card. `pkg:nvidia` is a recipe a machine builds
with `oath build plan:nvidia` and then makes present: the kernel
modules, its 64-bit userspace in `lib/` and its 32-bit in `lib32/`
(Steam), Vulkan's manifest, EGL's external platforms (Wayland, GBM, X)
in `/lib/nvidia`. A machine without it finds only Mesa's.

**The session comes back.** The two tmux servers that hold every
shell, the terminal's (socket `sola`) and Projects' (`sola-ws`), are
their own services, `svc:sola-tmux` and `svc:sola-tmux-ws` (`tmux -L
<socket> -f /home/.config/sola/tmux.conf -D`, in the foreground, which
`sola-session` wants), as NixOS gives each its own systemd user unit.
Started by an app instead, they were in `svc:sola-session`'s cgroup,
and PID 1 killed every shell whenever that service restarted. Now a
restart of `svc:sola-session` closes only the windows: it reopens the
apps in Sola's `SessionApps`
(`/home/.config/sola/state.yaml`), and they reattach to their live
tmux sessions with the programs, scrollback and layout as they were.
After a reboot the shells are new: the terminal rebuilds each saved
tab with its splits and working directories, and Projects opens each
project's panes in their paths. Panes take `WAYLAND_DISPLAY` and
`XDG_SESSION_TYPE` from the app that made the session (Sola's
`tmux.conf`, `update-environment`), since the servers start before the
compositor names its socket.

The `desk-sola` case (metal only: `qemu = false`) checks the services
and that the tmux servers and shells are outside `svc:sola-session`'s
cgroup, opens the terminal, Projects and Design (their call planes
answer `project.list`) and the browser unless they are open, and loads
`https://example.com` in a tab of its own. It closes only what it
opened: closing an app also drops it from `SessionApps`. Projects' tabs run
in tmux on socket `sola-ws`; on start it merges its status hooks into
`/home/.claude/settings.json`, `/home/.codex/hooks.json` and
`/home/.grok/hooks`, which call `curl` on its socket
(`/run/user/1/sola-ws-hooks.sock`). The agents' CLIs are not part of
the desk: Claude Code is a recipe in every catalog (R23), built on the
machine with `oath build plan:claude-code`.

## Sound, Bluetooth, the buses, KVM (built)

`svc:dbus-session` (`dbus-daemon --session` on `/run/user/1/bus`),
`svc:pipewire`, `svc:wireplumber` and `svc:pipewire-pulse` run as
`home`; `svc:sola-shell` wants the last two. PID 1 makes `/dev/snd/*`
root:home 0660 ([devices.md](devices.md)). The shell's volume menu runs
`wpctl` and `pw-dump`; Chromium opens `libpulse.so.0` and finds
pipewire-pulse at `/run/user/1/pulse/native`. The bus is there for
Chromium (MPRIS, notifications), at-spi and WirePlumber's optional
parts; libdbus itself is a library CEF links.

Bluetooth is BlueZ 5.87 on the system bus. `svc:dbus-system`
(`dbus-daemon --system` on `/run/dbus/system_bus_socket`, as `void`,
R36; the desk's `host:local.env` names it in `DBUS_SYSTEM_BUS_ADDRESS`,
since 2026-10-30, for clients that otherwise look under `/var/run`, as
zbus does) reads its own policy and BlueZ's, from `pkg:bluez`'s slot;
`svc:bluetooth` runs `bluetoothd` as root (its state in
`/var/lib/bluetooth`). `bluetoothctl` pairs and connects. PipeWire's
Bluetooth plugin plays and records through it (A2DP with SBC; HSP and
HFP of its own), and WirePlumber, which wants the system bus, makes a
node for each device; a Bluetooth LE controller or keyboard is an input
device through `uhid`. No OBEX, mesh, MIDI or LE Audio codecs.

The KVM (R44). `svc:sola-kvm` runs `sola-kvm listen --bind
0.0.0.0:4242` (UDP, and TCP for the clipboard); the `desk` profile has it
off since 2026-10-30. The listener types through the compositor
(virtual pointer and keyboard), so it needs no device. Oath's half is
built: rivulet serves keyboard-shortcuts-inhibit (`be2a874d`, in desk
output `86dd0d999ce4`; `apps/rivulet/test/inhibit.sh`; sandboxed clients
cannot inhibit), so a client holding focus, a pointer lock and an
inhibitor gets every key and button but the builtin VT switch. Sola's
half is in Sola `25f2be60` (on Sola's `next`, not yet in a desk output;
tried on canto against itself on 2026-10-30): `sola-kvm server` captures
through a 2 px layer-shell strip on the shared edge (exclusive keyboard,
a one-shot pointer lock with a commit, relative motion, the inhibitor;
Ctrl+Alt+Esc or crossing back leaves), with no evdev; the link is
Noise_IK_25519_ChaChaPoly_BLAKE2s between static X25519 keys in
`~/.config/sola-kvm` (`sola-kvm pair home@<peer>` swaps them over SSH),
with replay protection, a rekey every two minutes, a handshake per
clipboard connection; neither side starts unpaired, and the old shared
`pair.key` is refused. On NixOS novus ran `sola-kvm server --input
evdev` with canto (10.0.0.3) to its right; on Oath it will run `sola-kvm
server` once its Sola is `25f2be60` or later and the two are paired.

## Signatures to watch

Seventeen keys were added for the desk (Joshua, 2026-10-18): seven for
rivulet's libraries, seven for Sola's (freetype, gperf, libevent,
pcre2, dbus, cups, alsa), and cmake, git (Junio C Hamano) and
libsndfile (David Seifert) for sound and building on the machine. Facts
that will matter at the next version bump:

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
- `holtmann` (Marcel Holtmann, RSA-4096, 2011, from kernel.org's
  pgpkeys.git) signs BlueZ and SBC; kernel.org signs the uncompressed tar.
- Mesa 26.3 is signed by Dylan Baker (Ed25519
  `71C4B75620BC75708B4BDB254C95FAAB3EB073EC`), not `engestrom`. Mesa's
  published keyring holds a copy that expired 2020; keyserver.ubuntu.com
  has it extended.
- `expat` expires 2026-12-28; `ser` 2027-04-12. libdrm's and
  wayland-protocols' release managers rotate (Samuel Pitoiset, Jonas
  Ådahl signed earlier releases).
- `gmp` (Niels Möller) had expired before it signed nettle 4.0.
- `libevent`'s signing subkey expires 2027-06-16; `dbus`'s 2028-07-27.
  CEF moves on: a bump is a new CEF commit, Chromium tarball and the
  toolchain pins its DEPS names in `pkgs/cef` (a build of several hours),
  and Sola's browser and Design are stale (the build warns) until Sola
  builds them against it.

For rivulet's libraries, twelve sources have no signature the verifier
can check, each saying why in its `pkg.toml`: libffi, libudev-zero,
libxkbcommon, seatd, libepoxy, mako and PyYAML publish none; libinput (since 1.19.4)
and hwdata sign only git tags; pixman signs only an inline-signed sha512
list; MarkupSafe and packaging have PEP 740 attestations (Sigstore DSSE),
which `sigstore.rs` does not read. Teaching the verifier those two
formats would sign three of them. Forge tag archives (libinput,
libudev-zero, libxkbcommon, hwdata) can be regenerated with new bytes;
a sha256 mismatch on one of those is not by itself an attack. Sola's
stack adds: GNOME's tarballs (glib, at-spi2-core, pango; sha256sum
files only), Mozilla's (nspr, nss; SHA256SUMS only), GitHub release
assets (tmux, harfbuzz, fribidi, the fonts),
fontconfig's GitLab package, cairo's inline-signed sha256sum (the
pixman format), and CEF (Google's Chromium tarball with an unsigned
`.hashes` file, the toolchain packages its DEPS pins by sha256, and a
GitHub archive of CEF's commit). Sound
and fonts add: PipeWire's and WirePlumber's GitLab tag archives
(lightweight tags), Lua (sha256 on lua.org) and meson's wrapdb patch,
PulseAudio's tarball (a sha256sum; only its git tag is signed), and
Noto's files from lightweight git tags.

## Known defects

- Sola's shell has no GL context in QEMU (softpipe gives wgpu no GLES 3
  context), so Sola runs only on a machine with a GPU Mesa drives; the
  `desk-sola` case is metal only.
- libudev-zero marks a sound card `SOUND_INITIALIZED` when it has a
  control device, in enumeration (upstream PR 69) and, since 2026-10-27,
  in monitor events (Oath's second patch), which PID 1's `change` on the
  card makes whole; a hot-plugged USB card is untested.
- NVIDIA's userspace has run in QEMU only (no card): the desk on
  novus's RTX 3090 Ti is untested until it boots the trial stick.
- Mesa drives AMD GPUs only (radeonsi, RADV) and softpipe: Intel's iris
  and ANV need Mesa's OpenCL C compiler, which needs LLVM and Clang
  (R17 took them out). On an Intel GPU (buccina's) Sola's shell has no
  GL context.
- `PKG_CONFIG_PATH` is a fixed list in the `desk` profile: a package
  added later is not on it until the list is edited.
- The desk image carries all of `pkg:gcc` (393 MB) because Mesa loads
  libstdc++ and libgcc_s; a runtime-only split of GCC's libraries would
  save it.
- Xwayland admits local clients by user name (`si:localuser`), so the
  desk user needs an `/etc/passwd` entry, and wlroots' shm buffers need
  `/dev/shm`. A machine has both (PID 1 mounts `/dev/shm`); the build
  sandbox has neither, so `sandbox.sh` makes them.
- libudev-zero always reports devices as initialized and lacks udev's
  rule-made properties (`ID_INTEGRATION`, `LIBINPUT_DEVICE_GROUP`,
  calibration), so libinput falls back to its quirks.
- rivulet keeps three upstream River bugs (see
  [apps/rivulet/README.md](../../apps/rivulet/README.md)).

## Target

- **Sola's other apps** when Joshua wants them (R21).
- **Steam:** its own spec, [steam.md](steam.md) (R29).
