# Network and SSH

## The link

There is one object: `net:net0`. PID 1 renames the machine's only
non-loopback NIC to `net0`. Do not hunt `eth0` or `ens3`. Do not admin
with `ip` / `ifconfig` / `dhclient`; apply converges with `/bin/ip`
and `/bin/udhcpc`.

| Id | Default |
|----|---------|
| `net:net0` | `up: true`, `ipv4: dhcp` |

```
oath get net:net0
oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
oath apply
oath set net:net0 ipv4=dhcp
oath apply
```

Downing the link or changing the address over SSH cuts your session.
Serial still works.

On QEMU, user networking hands out `10.0.2.15` by DHCP and forwards
host `127.0.0.1:2222` to guest port 22 (`OATH_SSH_PORT`).
`OATH_BRIDGE=br0` attaches the guest to a host bridge instead (needs
`qemu-bridge-helper` and `/etc/qemu/bridge.conf`; no port forward).

Known gap: DHCP runs once when `net:net0` converges. It is not a
supervised service, so the lease is never renewed.

## SSH

SSH login is **home** (uid 1, `HOME=/home`, shell `/bin/sh`). Root SSH
is denied and passwords are off. `sudo` has no password. Serial is
root when `svc:serial` is enabled. There is **no private key in the
image**.

Host keys are generated on first boot under `/oath/ssh/`. Login keys
are `ssh:local` `authorized` (OpenSSH public key lines); apply writes
`/home/.ssh/authorized_keys`.

```
oath get ssh:local
oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
oath apply
```

Empty `authorized` means nobody can log in over SSH. Undo restores the
last keys. Replacing the list with keys you do not hold locks you out;
apply does not ask for `--confirm` and nothing rolls it back.

Where the first keys come from:

- `cargo make install` writes the build host's public keys
  (`~/.ssh/*.pub`, default identities without a `.pub`, `ssh-add -L`,
  or `OATH_SSH_PUBKEY`) into `ssh:local`.
- `cargo make run` / `up` / `start` pass the same keys to the QEMU
  guest, and PID 1 adds them to `ssh:local` at boot.

`svc:sshd` is dropbear (`pkg:dropbear`, not removable). The same
package ships `/bin/ssh` and `/bin/ssh-keygen` (OpenSSH client),
`/bin/sftp-server`, `/bin/scp`, and `/bin/dbclient`. Guest
`/etc/ssh/ssh_config` is `StrictHostKeyChecking accept-new`. `scp`
and `sftp` as `home` work:

```
scp ./file home@host:/home/file
cargo make ssh                # QEMU: ssh -p 2222 home@127.0.0.1
cargo make ssh -- ls /        # extra ssh(1) args
```
