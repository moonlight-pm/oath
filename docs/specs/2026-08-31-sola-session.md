**Date:** 2026-08-31
**Status:** target (freeze)
**Implementation:** shipped (session manager)
**Dogfood:** QEMU probe `sola.session` (2026-08-31)
**Gaps:** kit apps still out (launcher entries point at `/bin` but
binaries are not packed); kvm, browser, mail, terminal, …; no dbus;
software GL; Oath-as-dev-host is T24
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Sola session manager on Oath (`svc:sola-session`)

T23 packed bus, call, the River bridge, and the shell. The launcher
emits `LaunchApp`; nothing owned that spawn. This freeze is that
owner — **not** kit apps, **not** a nested process manager.

---

## Locks this freeze owns

- **Fifth ELF in the one `pkg:sola` blob** (T24). Store
  `/oath/store/pkg/sola/`; `/bin/sola-session` via the symlink farm.
  Do not split the blob. Do not add `pkg:sola-session`.
- **`svc:sola-session`** — `/bin/sola-session`. `wants`:
  `svc:sola-bus`, `svc:sola-call`. `restart=on-failure` (Quit Sola
  exits 0 and stays down, same as the other session `svc:*`).
- **PID 1 remains the only supervisor.** `sola-session` launches
  *user apps* (direct spawn on Oath; no systemd). It is not
  `crates/sola`. Do not pack or start the nested PM.
- **Dual-mode seat** (T24): logs under `/oath/log` (not
  `/opt/sola/log`); child `WAYLAND_DISPLAY` from a live socket when
  `sola-wayland` is absent; launcher builtins use `/bin` on Oath and
  `/opt/sola/bin` on NixOS.
- Probe stays headless. gtk menubar is human `DISPLAY`.

---

## Courage test (this slice)

On the QEMU appliance:

1. `oath get svc:sola-session` actual running.
2. `pidof sola-session` non-empty. Probe `sola.session`.
3. `pgrep -x sola` empty.
4. Session stack from T23 still up. Serial and SSH still work.

---

## Out

- Kit apps (`sola-kit`, settings, terminal, browser, mail, …)
- `sola-kvm`, XWayland, Steam, udevd, dbus
- Nested `crates/sola`
- Splitting `pkg:sola`
