# host

The machine itself. There is exactly one object: `host:local`.

**When to use:** change the hostname; reboot or halt; set the
environment every service gets; set the local timezone; name other
machines the network's DNS does not; say where packs come from.

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
  to `run` so the box does not loop. A power change on its own takes no
  generation (nothing for undo); plain `reboot` / `poweroff` take the
  same path.
- `env` — `NAME=value` map. PID 1 puts these in every `svc` spawn.
  Root-owned `/etc/profile` exports them for logins. Do not write
  `$HOME/.profile`. Empty in the `base` profile.
- `timezone` — POSIX TZ string (`MST7MDT,M3.2.0,M11.1.0`). Empty is UTC.
  Every service and every login gets `TZ` from this, unless `env` sets
  `TZ` itself. The system clock and logs stay UTC.
- `hosts` — other machines by name: a map of host name to one IPv4 or
  IPv6 address. `/etc/hosts` is localhost, this `hostname`, then these,
  rewritten on every apply and boot, so do not edit it. A name is
  letters, digits, `-` and `.`; `localhost` and this hostname are
  refused (`/etc/hosts` has them). `set` replaces the whole map. Empty
  in the `base` profile. Prefer the network's DNS when it can serve the
  name: it covers every machine at once.
- `origins` — where this machine fetches a pinned pack it does not
  have, in order: a list of `http://` or `https://` prefixes, each tried
  as `{origin}/pkg/<name>/<hash>.tar` until one serves the pinned tree
  (the hash decides, so any origin may serve any pack). Used by apply
  for a `pkg` with a `hash` and no `url`, and by `oath build` for a
  missing build need. A pkg's own `url` wins. Packs record no location;
  the machine's log does (`/oath/log/oath.jsonl`, `"fetched"`). The `base`
  profile lists the default origin, `https://store.oath.moonlight.pm`;
  put a LAN mirror first to prefer it. `set` replaces the whole list.
- `profile` — the seed profile this machine was made from (`base`,
  `desk`). `oath update` takes the newest output published for it,
  `{origin}/out/<profile>.json`. The image sets it; empty takes no
  updates.
- `publishers` — the keys whose signed outputs `oath update` takes:
  `ssh-ed25519 AAAA… name` lines, one per build host. The `base`
  profile lists buccina's. A person adds a key; an agent does not.

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

Another machine on the LAN by name:

```
oath set host:local hosts='{"novus": "10.0.0.5"}'
oath apply host:local
getent hosts novus
```

A LAN mirror before the default origin:

```
oath set host:local origins='["http://10.0.0.5:8080", "https://store.oath.moonlight.pm"]'
oath apply host:local
```

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
