**Date:** 2026-09-01
**Status:** target (freeze)
**Implementation:** shipped (seventh ELF + CEF helper in `pkg:sola`)
**Dogfood:** canto live (`/bin/sola-browser`, CEF zygote + network
service; helper ready). QEMU probe after next `cargo make build`.
**Gaps:** `sola-wrapper` out; workspaces is T29; no dbus-daemon;
software GL / SwiftShader; Oath-as-dev-host is T24; xkb compose locale
still C.UTF-8 (XCOMPOSEFILE + river `XKB_CONFIG_ROOT`)
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Kit browser on Oath (`sola-browser`)

T26 packed `sola-terminal`. The launcher already lists Browser at
`env::bin_path("sola-browser")`. This freeze packs that ELF and the
**CEF tree it links** — **not** `sola-wrapper`, **not** the rest of
the kit, **not** a split of `pkg:sola`.

CEF is the hard payload: `libcef.so` plus Resources, and a glib/nss/X11
dlopen closure. Chromium’s sandbox is already off (`no_sandbox`). Do
not invent a second Unix user to please it (T9).

---

## Locks this freeze owns

- **Seventh ELF in the one `pkg:sola` blob** (T24). Store
  `/oath/store/pkg/sola/`; `/bin/sola-browser` via the symlink farm.
  Do not add `pkg:sola-browser` or `pkg:cef`. Do not split the blob.
- **CEF is a helper in that same blob**, like tmux. Layout:
  `/oath/store/pkg/sola/cef/Release` + `…/cef/Resources` (cache
  shape). Native NEEDED libs land in `pkg:sola/lib`. Guest rpath
  includes `cef/Release`. The host `~/.cache/sola/cef-<pin>` is the
  pack source (`cargo make install-cef`); do not git the binaries.
- **Runtime CEF dir** (dual-mode): `SOLA_CEF_DIR`, then
  `<install-prefix>/cef` next to `bin/` / `libexec/`, then the
  compile-time cache path. Do not bake `/home/…/.cache` into the
  guest.
- **PID 1 does not supervise the browser.** `sola-session` launches
  it (direct spawn). No new `svc:*`.
- **libdbus is a library, not a bus.** Pack `libdbus-1` if CEF
  NEEDs it. Do not start `dbus-daemon`. No udevd.
- Probe stays headless. gtk is human `DISPLAY`. Courage can prove
  the binary, `libcef.so`, and a live pid when RAM allows.

---

## Courage test (this slice)

On the QEMU appliance (and canto, once copied):

1. `test -x /bin/sola-browser`.
2. `test -f /oath/store/pkg/sola/cef/Release/libcef.so`.
3. After the session stack is up: spawn `/bin/sola-browser`;
   `pidof sola-browser` is non-empty. `pgrep -x sola` stays empty.
4. Serial and SSH still work.

---

## Out

- `sola-wrapper`, mail, settings, kit storybook, workspaces, …
- `pkg:sola-browser` / `pkg:cef` / splitting `pkg:sola`
- Nested `crates/sola`, udevd, dbus-daemon
- A second Unix user (T9)
- Rebuilding CEF with codecs on the Oath host (pack the operator
  cache as-is)
