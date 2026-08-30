**Date:** 2026-08-30
**Status:** target (freeze)
**Implementation:** partial
**Dogfood:** QEMU probe — SSH login / deny / undo / reboot; net ping
**Gaps:** LAN bridge not dogfooded here (no host br0); dhcp oneshot not a svc
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# SSH and DHCP

Extends [2026-08-29-net.md](2026-08-29-net.md). Same verbs. Serial stays
the console. Root is still the owner (T9). No second Unix user.

---

## Locks this freeze owns

- **No baked login private key** in the image. Host SSH keys are
  **generated on first converge** under `/oath/ssh/` (in generations).
  Owner login is **public keys in the catalog**.
- Kind **`ssh`**, object **`ssh:local`**. Field `authorized`: list of
  OpenSSH public key lines. Apply writes `/root/.ssh/authorized_keys`
  as a side effect.
- **`svc:sshd`**: dropbear in the foreground (`-F -s -r
  /oath/ssh/host_ed25519`). Password auth off. `pkg:dropbear` is
  sealed (not removable).
- **`net:net0` `ipv4`** may be a CIDR **or** `dhcp`. DHCP uses busybox
  `udhcpc` and `/usr/lib/oath/udhcpc.script`. Actual may record
  `lease` (not a desired field).
- QEMU default: **user** net + **hostfwd** `127.0.0.1:$OATH_SSH_PORT`
  (default 2222) → guest 22. Optional **`OATH_BRIDGE=br0`**: bridge
  netdev, no hostfwd (guest DHCP on the LAN). Probe always uses user
  net.
- Network install later: the installer **injects owner public keys**
  into `ssh:local`; the target **generates host keys**. Still no
  image-wide private key.
- No glibc OpenSSH. No `dev` kind.

---

## Courage test

1. Boot. `net:net0` is up (dhcp or static). Ping the default gateway.
2. Host generates an ed25519 key. `oath set ssh:local --from-json
   '{"authorized":["ssh-ed25519 …"]}'` · `oath apply`.
3. From the QEMU host: `ssh -p 2222 -i <key> root@127.0.0.1` runs a
   command.
4. Empty `authorized` · apply → SSH auth fails. `undo` restores the
   key. Reboot: key and host key persist.

---

## Out of this freeze

- Fetching packages over the net
- DHCP as a long-running `svc` (v0 is oneshot `udhcpc` on converge)
- Creating a host bridge
- A second Unix user
