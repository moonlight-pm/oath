# Network

v0 has one object: `net:net0`. QEMU user networking + virtio-net. The
guest NIC is renamed to `net0`. Serial is still how you log in. There
is no SSH.

## What ships

| Id | Default | Notes |
|----|---------|-------|
| `net:net0` | `up: true`, `10.0.2.15/24`, gateway `10.0.2.2` | QEMU user net default (static). `ipv4=dhcp` for a LAN/bridge. |

```
oath ls --kind net
oath get net:net0
ping 10.0.2.2
oath set net:net0 up=false
oath apply
oath undo
```

Do not admin with `ip` / `ifconfig` / `dhclient`. `/bin/ip` is how
apply converges. Downing the link is `mutate` (serial still works).

DHCP:

```
oath set net:net0 ipv4=dhcp
oath apply
```

`cargo make run` with **`OATH_BRIDGE=br0`** attaches virtio-net to that
host bridge (needs qemu-bridge-helper + `/etc/qemu/bridge.conf`).
Default is user net + SSH hostfwd, not a LAN bridge. This host may not
have a bridge; the env is the hook.
