**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing list + live-install script; guest cargo of
the full kit **failed** on canto (`alsa-sys` / empty `.pc` farm);
`sola-arcade` built (T37)
**Dogfood:** packing list is in; `/bin/sola-arcade` on canto (T37).
Other T36 names **not** linked. QEMU after next `cargo make build`.
**Gaps:** guest `cargo build` of spotify/librespot died on `alsa.pc`.
**T37** adds Steam / gamescope / Xwayland / mesa as separate `pkg:*`. Spotify
MPRIS wants a session bus — no `dbus-daemon`. Zig `cc` must drop rustc
`-fuse-ld=lld`, cc-rs `--target=`, and `--dynamic-linker` on `-c`.
First full oath-sola inner loop is still out.
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Remaining kit apps on Oath (`pkg:sola`)

T26–T29 packed Terminal, Browser, Workspaces, and KVM. The launcher
already lists the rest at `env::bin_path("sola-…")`. This freeze packs
those ELFs into the **same `pkg:sola` blob** — **not** a split, **not**
new `svc:*`. Steam/gamescope/Xwayland/mesa are T37 (`pkg:*`, not this blob).

---

## Locks this freeze owns

- **More ELFs in the one `pkg:sola` blob** (T24). Store
  `/oath/store/pkg/sola/`; `/bin/<name>` via the symlink farm. Do not
  add `pkg:sola-mail` / `pkg:sola-spotify` / … Do not split the blob.
- **The set:**

  | `/bin` | Role |
  |--------|------|
  | `sola-settings` | Settings |
  | `sola-monitor` | Monitor |
  | `sola-kit` | Kit storybook |
  | `sola-preview` | Preview |
  | `sola-paint` | Paint |
  | `sola-mail` | Mail (IMAP/SMTP) |
  | `sola-arcade` | Arcade (library UI) |
  | `sola-scope` | Scope |
  | `sola-spotify` | Spotify (Pulse via `pkg:pipewire`) |
  | `sola-wrapper` | Website apps (CEF; same tree as browser) |

  Session stack, terminal, browser, workspaces, `solactl`, kvm, and
  tmux stay as they are.
- **`sola-wrapper` uses the packed CEF** (`SOLA_CEF_DIR`, guest rpath
  includes `cef/Release`). Same helper as T28. No `pkg:cef`.
- **`sola-spotify` talks Pulse** at `$XDG_RUNTIME_DIR` (pipewire-pulse).
  Audio is `pkg:pipewire`, not this blob. Guest rpath may search
  `pkg:pipewire/lib`.
- **PID 1 does not supervise kit apps.** `sola-session` launches them
  (direct spawn). No new `svc:*`.
- **Arcade ELF in this freeze.** Launching a title is T37
  (`pkg:gamescope` / `pkg:steam` / `pkg:xwayland`).
- **libdbus is a library.** Do not start `dbus-daemon` for MPRIS.
- **Canto fill** is `image/install-sola-kit.sh` (guest `cargo build`
  from the Sola tree + patchelf into the live store). Image pack is
  the same ELF list in `relocate-sola.sh` / `SOLA_KIT_ELFS`.

---

## Courage test (this slice)

On canto (and QEMU after rebuild):

1. `test -x /bin/sola-settings` and the other nine names above.
2. `test -x /bin/sola-wrapper`. `SOLA_CEF_DIR` still points at
   `/oath/store/pkg/sola/cef`.
3. After the session stack is up: spawn `/bin/sola-settings`;
   `pidof sola-settings` is non-empty. `pgrep -x sola` stays empty.
4. Serial and SSH still work.

---

## Out

- `pkg:sola-mail` / splitting `pkg:sola`
- Nested `crates/sola`, udevd, dbus-daemon
- XWayland / Steam / gamescope as *this* freeze (T37)
- A second Unix user
- Self-watch inner loop (apply a tree you built)
