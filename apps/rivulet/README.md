# rivulet: River ported to C (R19)

Oath's compositor. A hard fork of River 0.4.5 (upstream `f6d9617`, kept
in `forks/river` until rivulet runs Sola on Oath) with Oath's five
patches, ported from Zig to C on wlroots 0.20 on 2026-10-18 and named
rivulet. It serves River's protocols, the same globals at the same
versions, so `sola-river` drives it unchanged. GPL-3.0-only
([LICENSE](LICENSE)); every file names the Zig file it came from.

## Layout

| Path | What |
|------|------|
| `src/<file>.c` | one per Zig file: `Window.zig` → `window.c`, `XdgToplevel.zig` → `xdg_toplevel.c` |
| `src/rivulet.h` | the one header every `.c` includes: libraries, protocol headers, forward declarations, then the area headers |
| `src/{util,config,output,core,window,cursor,seat,server}.h` | types and prototypes by area, in include order |
| `protocol/` | River's six protocols, the three wlr ones its headers need, and wlr's virtual pointer for `test/inhibit.c` |
| `Makefile` | `make` → `build/rivulet`; `BUILD=<dir>` builds elsewhere; `make install DESTDIR= PREFIX=` |
| `dev.nix` | build host only: the shell to build and test on novus (Sola's nixpkgs pin) |
| `test/headless.sh` | a headless run driven by `sola-river`, and a comparison of two runs |
| `test/sandbox.sh` | the built `pkg:rivulet` in the build sandbox: headless, with an X client through Xwayland |
| `test/inhibit.sh`, `test/inhibit.c` | keyboard-shortcuts-inhibit end to end on an Oath build host: a window manager's chords against a client that captures input |

Names follow the Zig: type `Foo` is `struct rv_foo`, method `Foo.bar` is
`rv_foo_bar()`, fields keep their Zig names, a nested type takes its
field path (`Window.wm_scheduled` is `struct rv_window_wm_scheduled`).
An optional value is `has_x` plus `x`; a tagged union is a `tag` and an
anonymous union. Listeners are `struct wl_listener`, and whoever creates
a struct sets every `notify`.

## Build and test

```
nix-shell apps/rivulet/dev.nix --run 'make -C apps/rivulet -j8'
```

Sanitizers: `make BUILD=<dir> CFLAGS='-O1 -g -fsanitize=address,undefined'
LDFLAGS='-fsanitize=address,undefined'`.

The test needs `sola-bus`, `sola-call`, `sola-river` and `solactl` from
a checkout of [Sola](https://github.com/moonlight-pm/Sola). Build them
with `--manifest-path` from a directory outside this repo, because
Oath's `.cargo/config.toml` and Sola's both define a `make` alias and
Cargo refuses to merge them. Then:

```
SOLA_BIN=<dir> test/headless.sh run <compositor> <out>
test/headless.sh compare <out-upstream> <out-port>
```

A run types into a focused shell, clicks and scrolls, maps an Xwayland
`xterm`, a layer-shell wallpaper (`swaybg`), changes the output's mode
and scale, maps a fullscreen client, kills clients hard, opens six
windows at once and restarts `sola-river`. On 2026-10-18 the sanitizer
build and upstream River 0.4.5 gave the same 53 globals, the same steps,
window lists and output state, the same log lines apart from how many
manage and render sequences ran, and pixel-identical screenshots, with no
sanitizer reports. The one leak is upstream's: a `Seat` is never freed.

`pkgs/rivulet` builds this tree (`tree = "apps/rivulet"`); rebuild it
with `cargo make pkgs rivulet` after a change here, or the image ships
the locked product. `test/sandbox.sh` then runs that product in the
build sandbox: rivulet headless on pixman, and as its init an X client
(`xkbcomp`) that starts Xwayland and reads the keymap back.

## Where it differs from the Zig

- It is `rivulet`: the binary, `-version` (`0.1.0`), the init file
  (`$XDG_CONFIG_HOME/rivulet/init`), the nested output's title and app
  id, and its log lines. The protocols keep River's names. Sola's own
  process manager (`crates/sola`) starts a binary named `river`; on
  Oath it does not run, and PID 1 runs `svc:rivulet`.
- When wlroots cannot make a GPU renderer and `WLR_RENDERER` is unset,
  rivulet uses the pixman renderer instead of exiting (QEMU's virtio-gpu,
  a GPU Mesa has no driver for).
- The render-order hash is FNV-1a 64, not Blake3; only equality is used.
- With xkbcommon older than 1.12, keymaps compare through
  `xkb_keymap_get_as_string` instead of `…_string2`.
- It serves keyboard-shortcuts-inhibit-unstable-v1 (R44): while the
  surface with keyboard focus holds an inhibitor, keys skip the window
  manager's bindings (the builtin VT switch still runs), and when that
  surface also has the pointer locked or confined, buttons skip the
  pointer bindings. An inhibitor is active only while its surface has
  focus; sandboxed clients (security-context) do not see the global.
  `test/inhibit.sh` checks it against a window manager's chords.
- Log lines go to stderr through stdio. Zig's River writes stderr at its
  own offset, so in a file shared with Xwayland it overwrote lines.

Upstream bugs are kept until a test shows them: `XdgToplevel.configure`
compares bounds against the scheduled state (so bounds are never sent),
a decoration committed after its window closes trips an assert, and an
invalid keymap format leaks its fd.
