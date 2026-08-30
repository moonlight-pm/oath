# Plan — Phase 4 net canary

**Date:** 2026-08-29
**Status:** complete
**Proof:** probe `net.ping` / `net.down` / `net.undo_ping` /
`reboot.net_ping` (2026-08-30).
**Freeze:** [../specs/2026-08-29-net.md](../specs/2026-08-29-net.md)

No SSH. No DHCP. No `dev` kind. No pkg fetch.

---

- [x] Kind `net`: schema + Markdown; seed `net:net0` up with QEMU
      static address; INDEX one-liner.
- [x] QEMU `-netdev user` + `virtio-net-pci`. Load `failover`,
      `net_failover`, `virtio_net` in initramfs.
- [x] Converge: rename sole NIC to `net0`; `ip` addr/route; boot +
      apply + undo.
- [x] Probe: up/ping, down, undo, reboot persist.
- [x] Tests without QEMU (set/apply/undo `up`).
- [x] Manual when the probe passes.
