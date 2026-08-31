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
| [2026-08-30-wants-and-fetch.md](2026-08-30-wants-and-fetch.md) | `svc` wants + `pkg.url` fetch |
| [2026-08-30-display.md](2026-08-30-display.md) | virtio-gpu, `dev:card0`, gtk window |
| [2026-08-30-pkg-hosting.md](2026-08-30-pkg-hosting.md) | T20: `pkg.url` origin; Oath hosts as store |
| [2026-08-30-sola.md](2026-08-30-sola.md) | T21: Sola on Oath, River first |
| [2026-08-30-libinput.md](2026-08-30-libinput.md) | T22: libinput via libudev-zero, no udevd |
| [2026-08-30-oath-sola.md](2026-08-30-oath-sola.md) | T23: Sola session on Oath (`oath-sola`) |
| [2026-08-31-sola-dev.md](2026-08-31-sola-dev.md) | T24: Sola/app development layout (Nix host until Oath is) |
| [2026-08-31-sola-session.md](2026-08-31-sola-session.md) | T25: `svc:sola-session` LaunchApp owner |
| [2026-08-31-sola-terminal.md](2026-08-31-sola-terminal.md) | T26: first kit app (`sola-terminal` + tmux) |
| [2026-08-31-metal-canto.md](2026-08-31-metal-canto.md) | T27: canto metal canary (SSH+kexec) |

Active plan:
[../plans/2026-08-31-metal-canto-plan.md](../plans/2026-08-31-metal-canto-plan.md)
(T27). Completed:
[../plans/2026-08-27-qemu-skeleton-plan.md](../plans/2026-08-27-qemu-skeleton-plan.md) (Phase 1),
[../plans/2026-08-28-svc-loop-plan.md](../plans/2026-08-28-svc-loop-plan.md) (Phase 2),
[../plans/2026-08-28-pkg-canary-plan.md](../plans/2026-08-28-pkg-canary-plan.md) (Phase 3 canary),
[../plans/2026-08-29-pkg-base-plan.md](../plans/2026-08-29-pkg-base-plan.md) (Phase 3 base pkgs),
[../plans/2026-08-29-net-canary-plan.md](../plans/2026-08-29-net-canary-plan.md) (Phase 4 net),
[../plans/2026-08-30-ssh-dhcp-plan.md](../plans/2026-08-30-ssh-dhcp-plan.md) (SSH/DHCP),
[../plans/2026-08-30-devices-plan.md](../plans/2026-08-30-devices-plan.md) (devices),
[../plans/2026-08-30-wants-fetch-plan.md](../plans/2026-08-30-wants-fetch-plan.md) (wants + fetch),
[../plans/2026-08-30-display-plan.md](../plans/2026-08-30-display-plan.md) (display),
[../plans/2026-08-30-sola-river-plan.md](../plans/2026-08-30-sola-river-plan.md) (T21 River),
[../plans/2026-08-30-libinput-plan.md](../plans/2026-08-30-libinput-plan.md) (T22 libinput),
[../plans/2026-08-30-oath-sola-plan.md](../plans/2026-08-30-oath-sola-plan.md) (T23 session),
[../plans/2026-08-31-sola-session-plan.md](../plans/2026-08-31-sola-session-plan.md) (T25 session manager),
[../plans/2026-08-31-sola-terminal-plan.md](../plans/2026-08-31-sola-terminal-plan.md) (T26 sola-terminal).
