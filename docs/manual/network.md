# Network

v0 has one object: `net:net0`. QEMU user networking + virtio-net. The
guest NIC is renamed to `net0`. Serial is still how you log in. There
is no SSH.

## What ships

| Id | Default | Notes |
|----|---------|-------|
| `net:net0` | `up: true`, `10.0.2.15/24`, gateway `10.0.2.2` | Static. DHCP is not this kind. |

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
