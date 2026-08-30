# CURRENT — living product focus

**Only session handoff.** Update when priority, next slice, or known runtime
state changes. Full model: [docs/progress-model.md](docs/progress-model.md).
Capability maturity: [docs/capabilities.md](docs/capabilities.md).

**Decisions agents must ask about:**
[docs/open-questions.md](docs/open-questions.md).

**As of:** 2026-08-30

---

## Now

1. **Phase 4 SSH + DHCP shipped.** `ssh:local` catalog keys, dropbear,
   hostfwd 2222. `ipv4=dhcp` supported; default image is still slirp
   static. `OATH_BRIDGE` for a host bridge. Do not add `dev` or pkg
   fetch unless asked.
2. Do not add glibc runtime / installer unless CURRENT is updated.
3. Do not install to a real disk.

**Always allowed:** docs hygiene; tests; `cargo make build|run|probe`.

---

## Known dogfood state

| | **QEMU appliance** |
|--|---------------------|
| Role | Serial + SSH appliance (QEMU user net) |
| How | `cargo make build` then `probe` / `run` |
| Notes | `net:net0` + SSH pubkey login/undo/reboot. Manual: `docs/manual/`. Telemetry: `build/runs/<id>/`. |

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
- v0 kinds `host`, `svc`, `snap`, `pkg`, `net`, `ssh`. Sibling `@gen-N`
  at `/oath/run/fs`.
- Network: `net:net0` renamed NIC. Default static slirp
  `10.0.2.15/24`. `ipv4=dhcp` via udhcpc. `OATH_BRIDGE` optional.
- SSH: root, dropbear, **no baked private key**. Host keys under
  `/oath/ssh/`. Owner pubkeys in `ssh:local`. Serial still works.
- Packages: store `/oath/store/pkg/<name>/`; `/bin` is a symlink farm;
  `busybox`/`btrfs`/`oath`/`dropbear` not removable; hello is. No
  fetch in v0.
- Services: PID 1 converges `svc:*`. `svc:serial` is the console;
  `svc:sshd` is dropbear; `svc:hold` is the start/stop test.
- MIT, Copyright (c) Joshua Kifer.

---

## Pointers

- Manual: [docs/manual/README.md](docs/manual/README.md)
- Capabilities: [docs/capabilities.md](docs/capabilities.md)
- Freeze: [docs/specs/2026-08-30-ssh-and-dhcp.md](docs/specs/2026-08-30-ssh-and-dhcp.md)
- Plan: [docs/plans/2026-08-30-ssh-dhcp-plan.md](docs/plans/2026-08-30-ssh-dhcp-plan.md)
  (complete)
- Roadmap: Phase 4 (active; net + ssh dogfood)
