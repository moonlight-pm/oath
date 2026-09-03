# host

The machine itself. There is exactly one object: `host:local`.

**When to use:** change the hostname; reboot or halt the appliance;
set required process environment.

**When not:** do not invent extra host objects. Network and packages are
not this kind. Do not add a Unix user kind — the seat is always `home`.

## Fields

- `hostname` — Linux hostname. `mutate`. Survives reboot. Catalog is
  truth; `/etc/hostname` may be written as a side effect.
- `power` — `run` (steady), `reboot`, or `halt`. Changing away from
  `run` is **confirm** (`oath apply --confirm`). After a reboot apply,
  desired is set back to `run` so the box does not loop.
- `env` — `NAME=value` map. PID 1 injects these into every `svc`
  spawn. `/etc/profile` is a root-owned side effect. Do not write
  `$HOME/.profile`. Seed
  includes `GROK_DISABLE_AUTOUPDATER=1`.

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
