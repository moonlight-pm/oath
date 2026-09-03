# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-09-03

---

## Now

1. **Canto Sola session is up** (amdgpu Pitcairn, pixman compositor). Current
   panel: Philips 221V8L **1920×1080@75**. DualUp 2560×2880@30 works
   on that HDMI path (60 Hz needs the LG’s DisplayPort). Graphical
   boot is the white Oath mark on black (nothing else until Sola).
   **Canto EFI splash is up.** Boot graphics is layered (EFI GOP mark,
   then defer KMS that would steal that fb, River on black). Canto
   is one stack, not every machine. SI still flashes on KMS bind.
   Virtio SW cursor / `LIBGL_ALWAYS_SOFTWARE` only when a DRM card is
   virtio — Canto uses hardware cursors (sola-scope). Super+K overlay
   is card-sized (Sola `f8ff7871`). **T29:** `/bin/sola-workspaces` +
   `/bin/solactl` on canto (no git/grok yet). **T30:** `pkg:grok`
   identity locked (catalog owns the ELF; Grok does not self-update).
   **T31** on canto: `ssh home@canto` (uid 1, sudo ALL, no
   password). Graphical stack **off** (river/Sola/seatd disabled —
   crash-looped as `home` on amdgpu). Serial **off** (no UART; was
   restart-spamming tty0). Next: bring River/Sola back as `home`,
   pack `pkg:grok`, or amdgpu accel.
2. **T27 metal canary is in.** `ssh home@canto`. `host:local` canto,
   `net:net0` dhcp 10.0.0.3.
3. **T26 sola-terminal in.** **T28 sola-browser in** on canto (CEF
   zygote; helper ready). **T29 sola-workspaces + solactl in** on
   canto (no git/grok). Other kit apps still out.
4. **T24 identity locked** (one `pkg:sola` blob, apply/undo). Oath-as-dev-host
   **started**: Workspaces ELF is on canto. Git/grok/rustc still host-side.
   **T30** grok identity locked; ELF not packed. **T31** seat `home`
   locked (SSH home, serial root, sudo ALL no password, `/lib/oath`).
5. **T20 hosting locked**, not implemented.
6. Do not add a throwaway compositor. glibc runtime is allowed
   **only** as `pkg:glibc` for this payload (never in PID 1). No
   udevd. No nested Sola process manager. Do not write a real disk
   the operator did not name, or without `--confirm`.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe|install` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** | **canto (metal)** |
|--|---------------------|-------------------|
| Role | Serial + SSH + virtio-gpu appliance | First metal canary |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` | `ssh home@canto` (10.0.0.3) |
| Notes | `dev:card0` + gtk Sola menubar if DISPLAY, 1280×800 1:1 (`dev:kbd0` / `dev:mouse0`, no udevd). Virtio: pixman + SW cursor + `LIBGL_ALWAYS_SOFTWARE`. Menubar panels are card-sized (software GL). Window menu + Super+K from current Sola. Launcher Terminal is `/bin/sola-terminal` (tmux: `libresolv` + C.UTF-8; UI: SF Pro Text; mono: Iosevka Term Slab, Inter/JetBrains Mono fallbacks). Workspaces ELF after next `cargo make build`. Host SSH keys on up/start. Manual: `docs/manual/`. | GPT `/dev/sda` ESP+btrfs `@`. Dual Pitcairn (`1002:6810`) via amdgpu `si_support=1`. HDMI `card1` DP-10. **Now:** Philips 221V8L 1920×1080@75. DualUp native 2560×2880 is 30 Hz on HDMI (60 Hz on the LG’s DP). T31 seat `home`. River/Sola **disabled** until DRM-as-home is fixed. EFI splash: white mark on black at GOP 1920×1080 (`oath-efi` as BOOTX64). `/bin/sola-workspaces` + `/bin/solactl` packed (session not running). Magic Keyboard + Razer Taipan. `net:net0` dhcp 10.0.0.3. Kit fonts: SF Pro Text + Iosevka Term Slab. `/bin/sola-browser` + CEF in `pkg:sola`. |

```sh
nix-shell
cargo make build
cargo make probe
cargo make run
```

---

## Locked models

Do not re-litigate without an explicit decision.

- Oath: Linux kernel, own userspace, musl base, own PID 1.
- Catalog `/oath`, ids `kind:name`, `oath` is the only admin surface.
- v0 kinds `host`, `svc`, `snap`, `pkg`, `net`, `ssh`, `dev`. Sibling
  `@gen-N` at `/oath/run/fs`.
- Network: `net:net0` renamed NIC. Default static slirp
  `10.0.2.15/24`. `ipv4=dhcp` via udhcpc. `OATH_BRIDGE` optional.
- SSH: **home** only, dropbear `-w`, **no baked private key**. Host
  keys under `/oath/ssh/`. Owner pubkeys in `ssh:local` →
  `/home/.ssh/authorized_keys`. Serial is root. `home` has `sudo`
  ALL, no password. Unix name `home`, uid 1, `HOME=/home`.
- Packages: store `/oath/store/pkg/<name>/`; `/bin` is a symlink farm;
  `busybox`/`btrfs`/`oath`/`dropbear`/`glibc` not removable; `river`,
  `sola`, `hello`, and `fetchme` are. `pkg.url` wget canary. **T20:** no
  canonical archive; another Oath host’s store is a valid origin. Git
  is not the store. **T30:** `pkg:grok` is catalog-owned (not packed);
  Grok does not self-update.
- Services: PID 1 converges `svc:*` in `wants` order. `svc:serial` is
  the console; `svc:sshd` is dropbear; `svc:hold` wants serial;
  `svc:river` wants `svc:seatd`. Sola session: `svc:sola-bus` /
  `sola-call` / `sola-river` / `sola-shell` / `sola-session`.
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set
  is pixman River plus the Sola menubar (software GL, McMojave
  cursor), **1280×800 1:1** (`virtio-gpu-pci,xres/yres` + gtk
  `zoom-to-fit=off` + `GDK_SCALE=1` + sola-river
  `SOLA_OUTPUT_PICK=preferred`; `OATH_DISPLAY_WIDTH` / `HEIGHT`). Input is
  libinput via libudev-zero (`dev:kbd0` / `dev:mouse0`). No udevd.
  Path fallback in `forks/wlroots`. Metal BOOTX64 is `oath-efi`
  (native GOP, white mark on black); Linux does not paint fb.
- Sola on Oath: PID 1 is the only supervisor. River is `pkg:river` +
  `svc:river`. Session stack is T23 + T25 (`pkg:sola` + `svc:sola-bus` /
  `call` / `river` / `shell` / `session`; graphical stack as `home`). First kit app is T26
  (`/bin/sola-terminal` + tmux in that blob). T28 is `/bin/sola-browser`
  + CEF in the same blob. T29 is `/bin/sola-workspaces` + `solactl`
  in the same blob (no git/grok yet). **T30:** `pkg:grok` is the
  install; Grok does not update Grok; `$GROK_HOME` is state; not in
  the Sola blob. **T24:** one `pkg:sola` blob;
  development versions are apply/undo of the real objects (no second
  PATH, no nested PM). Oath-as-dev-host **started** (T29 Workspaces on
  canto); inner loop (git/grok/rustc) still Nix. glibc is sealed
  `pkg:glibc`. `forks/river` +
  `forks/wlroots` + `forks/sola`. Do not run `crates/sola`.
  First-party pkg sources under `apps/` (`hello`, `fetchme`).
  Sola-generic fixes cherry-pick to `moonlight-pm/Sola`, then merge
  back; Oath-compat stays on `oath-sola`. Merge Sola `master` into
  `oath-sola` regularly so the fork does not drift
  ([forks/README.md](forks/README.md)).
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-09-02-seat-home.md](docs/specs/2026-09-02-seat-home.md)
  (T31 seat `home` + `/lib/oath` + env). T30:
  [docs/specs/2026-09-02-pkg-grok.md](docs/specs/2026-09-02-pkg-grok.md)
  (`pkg:grok` identity). T29:
  [docs/specs/2026-09-02-sola-workspaces.md](docs/specs/2026-09-02-sola-workspaces.md)
  (sola-workspaces). T28:
  [docs/specs/2026-09-01-sola-browser.md](docs/specs/2026-09-01-sola-browser.md)
  (sola-browser). T27:
  [docs/specs/2026-08-31-metal-canto.md](docs/specs/2026-08-31-metal-canto.md)
  (metal canary, partial). T26:
  [docs/specs/2026-08-31-sola-terminal.md](docs/specs/2026-08-31-sola-terminal.md)
  (sola-terminal). T25:
  [docs/specs/2026-08-31-sola-session.md](docs/specs/2026-08-31-sola-session.md)
  (session manager). T24:
  [docs/specs/2026-08-31-sola-dev.md](docs/specs/2026-08-31-sola-dev.md)
  (identity; Oath-as-dev-host started). T23:
  [docs/specs/2026-08-30-oath-sola.md](docs/specs/2026-08-30-oath-sola.md)
  (session stack). T22:
  [docs/specs/2026-08-30-libinput.md](docs/specs/2026-08-30-libinput.md)
  (shipped).
- Plan: [docs/plans/2026-09-01-sola-browser-plan.md](docs/plans/2026-09-01-sola-browser-plan.md)
  (T28, complete). T27:
  [docs/plans/2026-08-31-metal-canto-plan.md](docs/plans/2026-08-31-metal-canto-plan.md)
  (complete). T26:
  [docs/plans/2026-08-31-sola-terminal-plan.md](docs/plans/2026-08-31-sola-terminal-plan.md)
  (complete). No T29 plan file (packed from the freeze).
- Hosting: [docs/specs/2026-08-30-pkg-hosting.md](docs/specs/2026-08-30-pkg-hosting.md)
  (T20 identity, not implemented)
- Roadmap: display canary in; River as `svc`; Sola session stack +
  session manager as `svc`; sola-terminal packed; sola-browser packed
  (canto; QEMU on next `cargo make build`); sola-workspaces packed
  (canto; QEMU on next build); `pkg:grok` identity locked, ELF not
  packed (T31 seat+env open); other kit apps not; Phase 6 metal
  canary (canto) dogfoodable, gaps remain
