**Date:** 2026-08-31
**Status:** target (freeze)
**Implementation:** shipped (sola-terminal + tmux helper)
**Dogfood:** QEMU probe `sola.terminal` / `sola.tmux` (2026-08-31)
**Gaps:** other kit apps (browser, mail, settings, …); no dbus;
software GL; Oath-as-dev-host is T24
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# First kit app on Oath (`sola-terminal`)

T25 packed `svc:sola-session` so the launcher’s `LaunchApp` has an
owner. The launcher already lists Terminal at `/bin/sola-terminal` on
Oath. This freeze packs that ELF — **not** the rest of the kit,
**not** a split of `pkg:sola`.

---

## Locks this freeze owns

- **Sixth ELF in the one `pkg:sola` blob** (T24). Store
  `/oath/store/pkg/sola/`; `/bin/sola-terminal` via the symlink farm.
  Do not add `pkg:sola-terminal`. Do not split the blob.
- **tmux is a helper in that same blob**, not a new kind and not
  `pkg:tmux`. `/bin/tmux` exists so the terminal’s PTY child can exec
  it. Terminfo for `xterm-256color` ships beside it.
- **PID 1 does not supervise the terminal.** `sola-session` launches
  it (direct spawn). No new `svc:*`.
- **tmux server without user systemd.** Loginless seats (and Oath)
  start the keepalive session with `tmux new-session -d` when
  `systemd-run --user` is absent. Do not invent a tmux `svc`.
- Probe stays headless. gtk is human `DISPLAY`. Courage can still
  prove the binaries and a live pid (River’s Wayland is up).

---

## Courage test (this slice)

On the QEMU appliance:

1. `test -x /bin/sola-terminal` and `test -x /bin/tmux`.
2. `tmux -V` prints a version.
3. After the session stack is up: spawn `/bin/sola-terminal`;
   `pidof sola-terminal` is non-empty. `pgrep -x sola` stays empty.
4. Serial and SSH still work.

---

## Out

- Other kit apps (`sola-kit`, settings, browser, mail, workspaces, …)
- `pkg:sola-terminal` / `pkg:tmux` / splitting `pkg:sola`
- Nested `crates/sola`, udevd, dbus
- A second Unix user (T9)
