# Plan — sola-browser on Oath

**Date:** 2026-09-01
**Status:** complete
**Proof:** probe `sola.browser_bin` / `sola.cef` / `sola.browser`
(plus T26 terminal + T25 session stack). gtk is host DISPLAY, not
this probe.
**Freeze:** [../specs/2026-09-01-sola-browser.md](../specs/2026-09-01-sola-browser.md)

- [x] T28 freeze: `sola-browser` + CEF helper in `pkg:sola`; no new
      kind; no `svc` for the app.
- [x] Sola-generic: resolve CEF dir at runtime (`SOLA_CEF_DIR` /
      `<prefix>/cef`); `open_url` uses `env::bin_path`.
- [x] Pack seventh ELF; copy CEF Release+Resources; walk libcef
      NEEDED + NSS extras; guest rpath; wrap `SOLA_CEF_DIR`.
- [x] Probe `sola.browser_bin` / `sola.cef` / `sola.browser`. Manual.
