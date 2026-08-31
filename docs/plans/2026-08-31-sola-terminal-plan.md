# Plan — sola-terminal on Oath

**Date:** 2026-08-31
**Status:** complete
**Proof:** probe `sola.terminal_bins` / `sola.tmux` / `sola.tmux_session` /
`sola.terminal`
(plus T25 session stack). gtk is host DISPLAY, not this probe.
**Freeze:** [../specs/2026-08-31-sola-terminal.md](../specs/2026-08-31-sola-terminal.md)

- [x] T26 freeze: `sola-terminal` + tmux helper in `pkg:sola`; no new
      kind; no `svc` for the app.
- [x] Sola-generic: `default_command` via `env::bin_path`; tmux server
      starts without `systemd --user`.
- [x] Pack `sola-terminal` as sixth ELF; relocate tmux + terminfo;
      `/bin` symlink farm.
- [x] Probe `sola.terminal` / `sola.tmux`. Tests. Manual.
