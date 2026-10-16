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
sudo oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
sudo oath apply --confirm
ssh home@192.168.1.20 sudo oath keep
```

`ipv4` is `dhcp` or `a.b.c.d/n`; `set` refuses anything else. Any
change to `up`, `ipv4` or `gateway` can cut your SSH session, so
`oath apply` refuses it (exit 3) without `--confirm`, and once applied
it is on probation: unless a **new** SSH session runs `sudo oath keep`
within 180 s (`--probation <secs>`), or if the machine reboots first,
PID 1 undoes it. A bad address rolls itself back. Serial still works.

On QEMU, user networking hands out `10.0.2.15` by DHCP and forwards
host `127.0.0.1:2222` to guest port 22 (`OATH_SSH_PORT`).
`OATH_BRIDGE=br0` attaches the guest to a host bridge instead (needs
`qemu-bridge-helper` and `/etc/qemu/bridge.conf`; no port forward).

With `ipv4: dhcp`, converge takes the first lease with a one-shot
`udhcpc`, and PID 1 keeps `udhcpc -f -i net0` running so the lease
renews. That client is PID 1's, not a `svc` object; it restarts with
backoff like a service, and its log is `oath logs net:net0`. Setting a
static address stops it. Each lease and renewal the client gets shows
in actual: `lease` is the address and `dhcp` has the router, DNS
servers, lease length and when it was renewed
(`oath get net:net0 --actual`).

## Time

`svc:time` is busybox `ntpd` against `0`–`3.pool.ntp.org` (decision
R12). At boot it steps a clock that is off by more than a second, then
slews. Its log has one `time:` line per event (a step, a new stratum,
losing sync, and every 11 minutes) plus ntpd's own complaints about
bad peers:

```
oath logs svc:time
… time: step stratum=16 offset=-102.361823s poll=32s
… time: stratum stratum=3 offset=-0.002908s poll=32s
```

To use other servers, change its `exec` (`oath set svc:time
--from-json '{"exec": […]}'` and `oath apply`).

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
sudo oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
sudo oath apply
```

Each key must start `ssh-`, `ecdsa-` or `sk-`. Adding a key is a plain
apply. Dropping one (or emptying the list) can lock you out, so it
needs `oath apply --confirm` and goes on probation: log in with a key
that is still listed and run `sudo oath keep`, or PID 1 puts the old
list back. Undo restores the last keys; it never rolls back the host
keys under `/oath/ssh/`.

Disabling or removing `svc:sshd`, or changing its `exec`, `user` or
`env`, also needs `--confirm` and goes on probation.

Where the first keys come from:

- `cargo make install` writes the build host's public keys
  (`~/.ssh/*.pub`, default identities without a `.pub`, `ssh-add -L`,
  or `OATH_SSH_PUBKEY`) into `ssh:local`.
- `cargo make run` / `up` / `start` pass the same keys to the QEMU
  guest, and PID 1 adds them to `ssh:local` (desired and applied) at
  boot. `cargo make check` passes its own throwaway key the same way.

`svc:sshd` is dropbear (`pkg:dropbear`, not removable: `dropbear` and
`dropbearkey` only). The OpenSSH client tools are `pkg:openssh`
(removable, built without OpenSSL): `/bin/ssh`, `/bin/scp`,
`/bin/sftp`, `/bin/ssh-keygen`, and `/bin/sftp-server`, which dropbear
runs for SFTP (removing `pkg:openssh` turns SFTP off). Guest `/etc/ssh/ssh_config` is
`StrictHostKeyChecking accept-new`. `scp` and `sftp` as `home` work:

```
scp ./file home@host:/home/file
cargo make ssh                # QEMU: ssh -p 2222 home@127.0.0.1
cargo make ssh -- ls /        # extra ssh(1) args
```
