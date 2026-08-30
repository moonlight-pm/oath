# Specs

Target freezes — normative desired shape, not progress trackers.

Prefer `YYYY-MM-DD-topic.md` with the header in
[`../progress-model.md`](../progress-model.md).

| Freeze | Role |
|--------|------|
| [2026-08-27-catalog-and-oath-surface.md](2026-08-27-catalog-and-oath-surface.md) | Catalog, objects, `oath` verbs, v0 kinds, apply/undo |
| [2026-08-28-packages.md](2026-08-28-packages.md) | Kind `pkg`, store + `/bin` symlinks, canary `pkg:hello` |
| [2026-08-29-pkg-base.md](2026-08-29-pkg-base.md) | `busybox` / `btrfs` / `oath` as sealed packages |
| [2026-08-29-net.md](2026-08-29-net.md) | Kind `net`, `net:net0` static QEMU slirp |
| [2026-08-30-ssh-and-dhcp.md](2026-08-30-ssh-and-dhcp.md) | `ssh:local` keys, dropbear, dhcp |
| [2026-08-30-devices.md](2026-08-30-devices.md) | `dev:*` inventory + Unix floor |

Active plan: none. Completed:
[../plans/2026-08-27-qemu-skeleton-plan.md](../plans/2026-08-27-qemu-skeleton-plan.md) (Phase 1),
[../plans/2026-08-28-svc-loop-plan.md](../plans/2026-08-28-svc-loop-plan.md) (Phase 2),
[../plans/2026-08-28-pkg-canary-plan.md](../plans/2026-08-28-pkg-canary-plan.md) (Phase 3 canary),
[../plans/2026-08-29-pkg-base-plan.md](../plans/2026-08-29-pkg-base-plan.md) (Phase 3 base pkgs),
[../plans/2026-08-29-net-canary-plan.md](../plans/2026-08-29-net-canary-plan.md) (Phase 4 net),
[../plans/2026-08-30-ssh-dhcp-plan.md](../plans/2026-08-30-ssh-dhcp-plan.md) (SSH/DHCP),
[../plans/2026-08-30-devices-plan.md](../plans/2026-08-30-devices-plan.md) (devices).
