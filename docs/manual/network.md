# Network and SSH

## The link

There is one object: `net:net0`. PID 1 gives the NICs 15 s to show
carrier and renames the first that does, in kernel-name order, to
`net0`; any others keep their kernel names, get no IPv4 address, and
are listed as `dev:net1` and up. A link named `wl*`, `wwan*`,
`docker*`, `br-*`, `virbr*` or `veth*` is never chosen, and no Wi-Fi
client ships, so `net0` is a wired link. With no carrier at boot the
boot goes on without `net0`, and `oath logs` says `net: no carrier on
…`. A cable plugged in later brings only IPv6 addresses, link-local
and from router advertisements, which sshd answers on; no `net0` and
no lease: reboot. Do not hunt `eth0` or `ens3`. Do not admin with `ip` / `dhcpcd`; apply
converges with `/bin/ip` and `/bin/dhcpcd`.

| Id | Default |
|----|---------|
| `net:net0` | `up: true`, `ipv4: dhcp` |

```
oath get net:net0
sudo oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
sudo oath apply --confirm
ssh home@192.168.1.20 sudo oath keep
```

`ping` works as `home`: PID 1 opens ICMP echo sockets to every group
(`net.ipv4.ping_group_range`), so it needs no setuid bit.

`up: false` takes the address and default route off and sets the link
down. `ipv4` is `dhcp` or `a.b.c.d/n`; `set` refuses anything else.
`gateway` is the default route of a static address; empty adds no
default route, and `dhcp` ignores it. `set` does not check it: a gateway
`ip` refuses fails the apply, which goes on probation like one that
succeeds.

Any change to `up`, `ipv4` or `gateway` can cut your SSH session, so
`oath apply` refuses it (exit 3) without `--confirm`, and once applied
it is on probation: unless a **new** SSH session runs `sudo oath keep`
within 180 s (`--probation <secs>`), or if the machine reboots first,
PID 1 undoes it. Serial still works. A bad address rolls itself back:
the spec and the lease return. Back on `dhcp`, by rollback or by
`oath set` and `oath keep`, a static address other than the lease and
any default route it set stay on `net0` beside the lease until the
next boot, and that route carries everything off-subnet, so reboot
after the move.

On QEMU, user networking hands out `10.0.2.15` by DHCP and forwards
host `127.0.0.1:2222` to guest port 22 (`OATH_SSH_PORT`).
`OATH_BRIDGE=br0` attaches the guest to a host bridge instead (needs
`qemu-bridge-helper` and `/etc/qemu/bridge.conf`; no port forward).

With `ipv4: dhcp`, converge takes the first lease with a one-shot
`dhcpcd -1`, and PID 1 keeps `dhcpcd -B … net0` running so the lease
renews (IPv4 only, no config file; Oath's hook
`/lib/oath/dhcpcd.hook` writes `/etc/resolv.conf`; the processes that
read the network run as `void`, chrooted in `/var/empty`). `ps` shows it as
`dhcpcd: net0 [ip4]`; `sudo dhcpcd -4 -n net0` makes it rebind now. That client is PID 1's, not a `svc` object; it restarts with
backoff like a service, and its log is `oath logs net:net0`. Applying a
static address stops it before the address changes; applying `dhcp`
again starts it. Each lease and renewal the client gets shows
in actual: `lease` is the address and `dhcp` has the router, DNS
servers, lease length and when it was renewed
(`oath get net:net0 --actual`).

## What listens

`svc:sshd` listens on port 22 on every IPv4 and IPv6 address, and is
the only TCP listener in the `base` profile; the DHCP client holds UDP
port 68 on its lease. No firewall ships: there is no `nft` or
`iptables`, so nothing filters what comes in. The DHCP client is IPv4
only, but the kernel takes IPv6 addresses from router advertisements
on every NIC that is up (`ip -6 addr`), and sshd answers on them.

## Names

Names resolve through `/etc/hosts`, then the DNS servers the lease
names (or that `/etc/resolv.conf` has): `/etc/nsswitch.conf` says
`hosts: files dns`. A LAN machine the DNS does not know goes in
`host:local.hosts`, which `/etc/hosts` lists after localhost and this
hostname:

```
sudo oath set host:local hosts='{"printer": "10.0.0.20", "nas": "10.0.0.12"}'
sudo oath apply host:local
getent hosts nas
```

A name in the map is letters, digits, `-` and `.`, at most 253, not
starting with `-` or `.`; its value is one IPv4 or IPv6 address.
`localhost` and this machine's `hostname` are refused: `/etc/hosts` has
them. `hostname` follows the same rule, at most 63.

`set` replaces the whole map, and refuses one that breaks these; `oath
undo` takes it back. Do not edit `/etc/hosts`: every boot, and every
apply or undo that changes `host:local`, writes it again. When the
network's DNS can serve a name, add it there instead, for every machine
at once. A machine-specific map belongs in the host's install overlay
(`hosts/<name>.json`).

A static address has no DNS servers of its own: `net:net0` has no field
for them, and only the DHCP client's hook writes `/etc/resolv.conf`. A
change to static leaves the file as the last lease wrote it; if no
lease ever wrote it, there is no file and only `/etc/hosts` resolves.
Put `nameserver <address>` lines in it by hand; nothing rewrites it
while the address is static.

`dig`, `host` and `nslookup` (BIND 9, `pkg:bind`) ask the DNS servers
in `/etc/resolv.conf` directly and never read `/etc/hosts`; `getent
hosts <name>` shows what programs get. `dig @<server> <name>` asks one
server, `dig +trace <name>` walks down from the root.

## Time

`svc:time` is `chronyd -d` against `0`–`3.pool.ntp.org` (decisions
R12, R18), configured on its command line, with no config file; after
start-up it runs as `void`, keeping only the rights to set the clock
and bind a low port (`CAP_SYS_TIME`, `CAP_NET_BIND_SERVICE`). In its
first three updates it steps a clock that is off by more than a second
(`makestep 1 3`), then slews. Its log is chrony's own:

```
oath logs svc:time
… Selected source 192.0.2.10 (0.pool.ntp.org)
… System clock wrong by -102.361823 seconds
… System clock was stepped by -102.361823 seconds
```

To use other servers, change its `exec` (`sudo oath set svc:time
--from-json '{"exec": […]}'` and `sudo oath apply`).

## SSH

SSH login is **home** (uid 1, `HOME=/home`, login shell `/bin/zsh`). Root SSH
is denied and passwords are off. `sudo` has no password. Serial is
root when `svc:serial` is enabled. There is **no private key in the
image**.

The host key is `/oath/ssh/ssh_host_ed25519_key`. `cargo make install`
and `oath install` write it. A machine that lacks it at boot converts
`/oath/ssh/host_ed25519`, a dropbear key, into it, or else makes a new
one. Login keys are `ssh:local` `authorized` (OpenSSH public key lines);
apply writes `/home/.ssh/authorized_keys`.

```
oath get ssh:local
sudo oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA… old","ssh-ed25519 BBBB… new"]}'
sudo oath apply
```

`set` replaces the whole list, so name every key that stays. Each key
must start `ssh-`, `ecdsa-` or `sk-`. Adding a key is a plain apply.
Dropping one (or emptying the list) can lock you out, so it needs `oath apply --confirm` and goes on probation: log in with a key
that is still listed and run `sudo oath keep`, or PID 1 puts the old
list back. Undo restores the last keys; it never rolls back the host
keys under `/oath/ssh/`.

Disabling or removing `svc:sshd`, changing its `exec`, `user` or
`env`, or moving `pkg:openssh`, the package it runs from, to another
hash also needs `--confirm` and goes on probation.

Where the first keys come from:

- `cargo make install` writes the build host's public keys
  (`~/.ssh/*.pub`, default identities without a `.pub`, `ssh-add -L`,
  and the file `OATH_SSH_PUBKEY` names) into `ssh:local`.
- `cargo make run` / `up` / `start` pass the same keys to the QEMU
  guest, and PID 1 adds them to `ssh:local` (desired and applied) at
  boot. `cargo make check` passes its own throwaway key the same way.

`svc:sshd` is OpenSSH's sshd (`pkg:openssh`, not removable, with
OpenSSL: RSA, ECDSA and ed25519 keys log in), which also gives `/bin/ssh`,
`/bin/scp`, `/bin/sftp`, `/bin/ssh-keygen` and `/bin/sftp-server`. Before a
login, sshd reads the network as `void`, chrooted in `/var/empty`. oath
writes `/etc/ssh/sshd_config` from `host:local` with `/etc/profile`, so a
command over SSH (`ssh host 'cmd'`, no login shell) gets `host:local` env
and `TZ` too, as of sshd's last start. The image's `/etc/ssh/ssh_config` is
`StrictHostKeyChecking accept-new`. `scp` and `sftp` as `home` work:

```
scp ./file home@host:/home/file
cargo make ssh                # QEMU: ssh -p 2222 home@127.0.0.1
cargo make ssh -- ls /        # extra ssh(1) args
```
