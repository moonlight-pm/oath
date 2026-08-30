# Plan — SSH + DHCP

**Date:** 2026-08-30
**Status:** complete
**Proof:** probe `ssh.login` / `ssh.denied` / `ssh.undo_login` /
`reboot.ssh_login` (2026-08-30).
**Freeze:** [../specs/2026-08-30-ssh-and-dhcp.md](../specs/2026-08-30-ssh-and-dhcp.md)

---

- [x] `net` `ipv4=dhcp` + udhcpc script; seed can stay slirp-friendly.
- [x] QEMU hostfwd 2222; `OATH_BRIDGE` uses `-netdev bridge`.
- [x] Pack sealed `pkg:dropbear` (dropbear + dropbearkey).
- [x] `ssh:local` authorized keys; generate `/oath/ssh/host_ed25519`;
      `svc:sshd`; mount `devpts`.
- [x] Probe: inject host-generated pubkey, SSH via 127.0.0.1:2222.
- [x] Tests without QEMU; manual when probe passes.
