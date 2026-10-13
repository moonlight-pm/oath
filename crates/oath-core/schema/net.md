# net

A network link. There is one object: `net:net0`. PID 1 renames the
machine's only non-loopback NIC to `net0`. Do not hunt `eth0` or
`ens3`. There is no `ifcfg`, NetworkManager, or `dhclient`.

**When to use:** bring the link up or down; switch between DHCP and a
static address.

**When not:** do not run `ip` / `ifconfig` by hand as admin.

## Fields

- `up` — link up with an address and default route, or down.
- `ipv4` — `dhcp` (the default) or a static CIDR (`192.168.1.20/24`);
  `set` refuses anything else.
- `gateway` — IPv4 default gateway for a static address. Ignored for
  `dhcp`.

Actual may carry `lease` (the address DHCP gave). It is not a desired
field.

Safety: `mutate`, but any change to `up`, `ipv4` or `gateway` can cut
off SSH, so `oath apply` needs `--confirm` for it. Apply snapshots, then
converges with `/bin/ip` and, for `dhcp`, `/bin/udhcpc`.

Known gap: DHCP runs once at converge. It is not a supervised
service, so a lease is not renewed.

## Example

```
oath get net:net0
oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
oath apply --confirm
oath set net:net0 ipv4=dhcp
oath apply --confirm
```

Downing the link over SSH cuts you off. Serial still works.
