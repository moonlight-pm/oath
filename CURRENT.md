# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-30

---

## Now

1. **Phase 4 net canary shipped** (`net:net0` static QEMU slirp).
   Do not add SSH, DHCP, `dev`, or pkg fetch unless asked.
2. Do not add glibc runtime / installer unless CURRENT is updated.
3. Do not install to a real disk.

**Always allowed:** docs hygiene; tests; `cargo make build|run|probe`.

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Phase 1–4 serial box with one static NIC |
| How | `cargo make build` then `probe` / `run` |
| Notes | Hostname + `svc:hold` + pkgs + `net:net0` ping/undo/reboot. Manual: `docs/manual/`. Telemetry: `build/runs/<id>/`. |

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
- v0 kinds `host`, `svc`, `snap`, `pkg`, `net`. Sibling `@gen-N` at
  `/oath/run/fs`.
- Network: `net:net0` static `10.0.2.15/24` via `10.0.2.2`. NIC
  renamed to `net0`. Serial is still how you log in.
- Packages: store `/oath/store/pkg/<name>/`; `/bin` is a symlink farm;
  `busybox`/`btrfs`/`oath` not removable; hello is. No new verbs; no
  fetch in v0.
- Services: PID 1 converges enabled/disabled `svc:*`. `svc:serial` is
  the console; `svc:hold` is the start/stop test process.
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-29-net.md](docs/specs/2026-08-29-net.md)
- Plan: [docs/plans/2026-08-29-net-canary-plan.md](docs/plans/2026-08-29-net-canary-plan.md)
  (complete)
- Roadmap: Phase 4 net (active; canary done)
