# host

The machine itself. There is exactly one object: `host:local`.

**When to use:** change the hostname; reboot or halt the appliance.

**When not:** do not invent extra host objects. Network and packages are
not this kind.

## Fields

- `hostname` — Linux hostname. `mutate`. Survives reboot. Catalog is
  truth; `/etc/hostname` may be written as a side effect.
- `power` — `run` (steady), `reboot`, or `halt`. Changing away from
  `run` is **confirm** (`oath apply --confirm`). After a reboot apply,
  desired is set back to `run` so the box does not loop.

## Example

```
oath schema host
oath get host:local
oath set host:local hostname=atlas
oath diff
oath apply
```

Reboot (owner asked):

```
oath set host:local power=reboot
oath apply --confirm
```

Halt turns the machine off (on QEMU, the host serial returns):

```
oath set host:local power=halt
oath apply --confirm
```
