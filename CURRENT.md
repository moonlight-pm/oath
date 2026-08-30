# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-30

---

## Now

1. **Display canary shipped** (`dev:card0`). Do not add a throwaway
   compositor or a placeholder `pkg:sola`. Graphical next is the
   compositor Sola actually uses, as `svc`, only when asked.
2. **T20 hosting locked**, not implemented: `pkg.url` is the origin;
   other Oath hosts may serve store bytes. No git store, no `repo` kind.
3. Do not add glibc runtime / River / Sola / installer unless CURRENT
   is updated. Do not install to a real disk.

**Always allowed:** docs hygiene; tests; `cargo make build|run|up|start|stop|ssh|probe` (`--build` on run/up/start).

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Serial + SSH + virtio-gpu appliance |
| How | `cargo make build` then `probe` / `run` / `up` / `start`+`ssh` |
| Notes | `dev:card0` + gtk if DISPLAY; host SSH keys injected on up/start. Manual: `docs/manual/`. |

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
  `busybox`/`btrfs`/`oath`/`dropbear` not removable; `hello` and
  `fetchme` are. `pkg.url` wget canary. **T20:** no canonical archive;
  another Oath host’s store is a valid origin. Git is not the store.
- Services: PID 1 converges `svc:*` in `wants` order. `svc:serial` is
  the console; `svc:sshd` is dropbear; `svc:hold` wants serial.
- Display: virtio-gpu `dev:card0`. gtk window when `DISPLAY` is set.
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-30-pkg-hosting.md](docs/specs/2026-08-30-pkg-hosting.md)
  (T20, not implemented)
- Display: [docs/specs/2026-08-30-display.md](docs/specs/2026-08-30-display.md)
  (complete)
- Plan: **none**
- Roadmap: display canary in; hosting identity locked; Wayland/Sola later
