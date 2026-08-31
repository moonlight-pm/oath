# Plan — Sola session on Oath (`oath-sola`)

**Date:** 2026-08-30
**Status:** complete
**Proof:** probe `sola.bus` / `sola.call` / `sola.bridge` / `sola.shell`
/ `sola.no_pm` (2026-08-31).
**Freeze:** [../specs/2026-08-30-oath-sola.md](../specs/2026-08-30-oath-sola.md)

- [x] T23 freeze: `oath-sola` private remote; `pkg:sola` + four `svc:*`;
      no nested PM; attach to PID 1’s River.
- [x] GitHub `moonlight-pm/oath-sola` (Sola `master`); submodule
      `forks/sola`.
- [x] Fork: live Wayland socket without `sola-wayland`; logs
      `/oath/log`; GPU env from `pkg:river`; `crates/sola` refuses
      to run when `/oath/INDEX.md` exists.
- [x] Pack glibc-linked `sola-bus` / `sola-call` / `sola-river` /
      `sola-shell` as `pkg:sola`. Inter fonts for the menubar.
- [x] Seed `pkg:sola` + the four `svc:*` (`wants` as freeze). PID 1
      sets `SOLA_NO_SELF_WATCH=1`. Do not pack `crates/sola`.
- [x] Probe: pids + sockets; `pidof sola` empty. gtk menubar is
      human DISPLAY, not probe. Tests. Manual.
