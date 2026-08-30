# net

A network link. v0 has one object: `net:net0`. The appliance NIC is
renamed to `net0`. Do not hunt `eth0` or `ens3`. There is no
`ifcfg`, NetworkManager, or `dhclient`.

**When to use:** bring the link up or down; set the static address.

**When not:** do not run `ip` / `ifconfig` by hand as admin. Do not
expect SSH. DHCP is not this kind (yet). Serial is how you reach the
box.

## Fields

- `up` — link up with address + default route, or down.
- `ipv4` — CIDR (appliance default `10.0.2.15/24`).
- `gateway` — IPv4 (appliance default `10.0.2.2`).

Safety: `mutate`. Apply snapshots, then converges with `/bin/ip`.

## Example

```
oath ls --kind net
oath get net:net0
oath set net:net0 up=false
oath apply
oath undo
```
