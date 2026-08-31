# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-31

---

## Now

1. **T23 session stack in.** `pkg:sola` + `svc:sola-bus` / `call` /
   `river` (bridge) / `shell` on the appliance. gtk menubar when
   `DISPLAY`. Full apps (`sola-session`, browser, …) out. Next: T20
   hosting (locked, not implemented) unless we continue Sola apps.
2. **T24 locked**, not implemented (Oath-as-dev-host). Keep one
   `pkg:sola` blob. Develop Sola on Nix until Oath is the host.
3. **T20 hosting locked**, not implemented.
4. Do not add a throwaway compositor. Do not install to a real disk.
   glibc runtime is allowed **only** as `pkg:glibc` for this payload
   (never in PID 1). No udevd. No nested Sola process manager.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Serial + SSH + virtio-gpu appliance |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` |
| Notes | `dev:card0` + gtk Sola menubar if DISPLAY (`dev:kbd0` / `dev:mouse0`, no udevd). Host SSH keys on up/start. Manual: `docs/manual/`. |

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
- SSH: root, dropbear, **no baked private key**. Host keys under
  `/oath/ssh/`. Owner pubkeys in `ssh:local`. Serial still works.
- Packages: store `/oath/store/pkg/<name>/`; `/bin` is a symlink farm;
  `busybox`/`btrfs`/`oath`/`dropbear`/`glibc` not removable; `river`,
  `sola`, `hello`, and `fetchme` are. `pkg.url` wget canary. **T20:** no
  canonical archive; another Oath host’s store is a valid origin. Git
  is not the store.
- Services: PID 1 converges `svc:*` in `wants` order. `svc:serial` is
  the console; `svc:sshd` is dropbear; `svc:hold` wants serial;
  `svc:river` wants `svc:seatd`. Sola session: `svc:sola-bus` /
  `sola-call` / `sola-river` / `sola-shell`.
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set
  is pixman River plus the Sola menubar (software GL, McMojave
  cursor). Input is libinput via libudev-zero (`dev:kbd0` /
  `dev:mouse0`). No udevd. Path fallback in `forks/wlroots`.
- Sola on Oath: PID 1 is the only supervisor. River is `pkg:river` +
  `svc:river`. Session stack is T23 (`pkg:sola` + `svc:sola-bus` /
  `call` / `river` / `shell`). **T24:** one `pkg:sola` blob; develop
  Sola on Nix until Oath is the host; then development versions are
  apply/undo of the real objects (no second PATH, no nested PM).
  glibc is sealed `pkg:glibc`. `forks/river` + `forks/wlroots` +
  `forks/sola`. Do not run `crates/sola`. First-party pkg sources
  under `apps/` (`hello`, `fetchme`).
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-31-sola-dev.md](docs/specs/2026-08-31-sola-dev.md)
  (T24 identity). T23:
  [docs/specs/2026-08-30-oath-sola.md](docs/specs/2026-08-30-oath-sola.md)
  (session stack). T22:
  [docs/specs/2026-08-30-libinput.md](docs/specs/2026-08-30-libinput.md)
  (shipped). T21:
  [docs/specs/2026-08-30-sola.md](docs/specs/2026-08-30-sola.md)
- Plan: [docs/plans/2026-08-30-oath-sola-plan.md](docs/plans/2026-08-30-oath-sola-plan.md)
  (complete).
- Hosting: [docs/specs/2026-08-30-pkg-hosting.md](docs/specs/2026-08-30-pkg-hosting.md)
  (T20 identity, not implemented)
- Roadmap: display canary in; River as `svc`; Sola session stack as
  `svc`; full apps not
