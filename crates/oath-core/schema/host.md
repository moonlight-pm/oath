# host

The machine itself. There is exactly one object: `host:local`.

**When to use:** change the hostname; reboot or halt; set the
environment every service gets; set the local timezone.

**When not:** do not invent extra host objects. Network and packages are
not this kind. Do not add a Unix user kind — the accounts are always
`root` and `home`.

## Fields

- `hostname` — Linux hostname. `mutate`. Catalog is truth;
  `/etc/hostname` and `/etc/hosts` are side effects.
- `power` — `run` (steady), `reboot`, or `halt`. Changing away from
  `run` is **confirm** (`oath apply --confirm`). The apply asks PID 1
  to shut down cleanly (stop services, sync, remount `/` read-only),
  then reboot or power off. After a reboot apply, desired is set back
  to `run` so the box does not loop. Plain `reboot` / `poweroff` take
  the same path without a generation.
- `env` — `NAME=value` map. PID 1 puts these in every `svc` spawn.
  Root-owned `/etc/profile` exports them for logins. Do not write
  `$HOME/.profile`. Empty in the `base` profile.
- `timezone` — POSIX TZ string (`MST7MDT,M3.2.0,M11.1.0`). Empty is UTC.
  Every service and every login gets `TZ` from this, unless `env` sets
  `TZ` itself. The system clock and logs stay UTC.

## Example

```
oath schema host
oath get host:local
oath set host:local hostname=atlas
oath diff
oath apply
```

Local time for services and logins:

```
oath set host:local timezone=MST7MDT,M3.2.0,M11.1.0
oath apply
```

Running services keep their old `TZ` until they restart.

Reboot (owner asked):

```
oath set host:local power=reboot
oath apply --confirm
```

Halt powers the machine off (on QEMU, the VM exits):

```
oath set host:local power=halt
oath apply --confirm
```
