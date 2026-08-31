# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-31

---

## Now

1. **T22 in.** River takes virtio keyboard/mouse (`dev:kbd0` /
   `dev:mouse0`, libudev-zero; path fallback in wlroots). gtk
   window should move a pointer. Next: `oath-sola` fork.
2. **T20 hosting locked**, not implemented.
3. Do not add a throwaway compositor or a placeholder `pkg:sola`.
   Do not install to a real disk. glibc runtime is allowed **only**
   as `pkg:glibc` for this payload (never in PID 1). No udevd.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Serial + SSH + virtio-gpu appliance |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` |
| Notes | `dev:card0` + gtk compositor if DISPLAY (`dev:kbd0` / `dev:mouse0`, no udevd). Host SSH keys on up/start. Manual: `docs/manual/`. |

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
  `hello`, and `fetchme` are. `pkg.url` wget canary. **T20:** no
  canonical archive; another Oath host’s store is a valid origin. Git
  is not the store.
- Services: PID 1 converges `svc:*` in `wants` order. `svc:serial` is
  the console; `svc:sshd` is dropbear; `svc:hold` wants serial;
  `svc:river` wants `svc:seatd`.
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set
  is pixman River. Input is libinput via libudev-zero (`dev:kbd0` /
  `dev:mouse0`). No udevd. Path fallback in `forks/wlroots`.
- Sola on Oath: PID 1 is the only supervisor. First slice is patched
  **River** as `pkg:river` + `svc:river`. glibc is sealed `pkg:glibc`.
  `forks/river` + `forks/wlroots` in; `oath-sola` not. First-party pkg
  sources under `apps/` (`hello`, `fetchme`).
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-30-libinput.md](docs/specs/2026-08-30-libinput.md)
  (T22 shipped). T21: [docs/specs/2026-08-30-sola.md](docs/specs/2026-08-30-sola.md)
- Plan: [docs/plans/2026-08-30-libinput-plan.md](docs/plans/2026-08-30-libinput-plan.md)
  (complete). Next: `oath-sola` fork.
- Hosting: [docs/specs/2026-08-30-pkg-hosting.md](docs/specs/2026-08-30-pkg-hosting.md)
  (T20 identity, not implemented)
- Roadmap: display canary in; River as `svc`; full Sola not
