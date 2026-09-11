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
  includes `GROK_DISABLE_AUTOUPDATER=1`, `SHELL=/bin/thoxa`, and
  (T35) `CC` / `CXX` / `AR` / `CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER`
  / `CMAKE_GENERATOR`. Do **not** put `TZ` here (T34).
- `timezone` — POSIX TZ for **display** (Sola clock, seat apps). Empty
  is UTC. Seed is US Mountain `MST7MDT,M3.2.0,M11.1.0`. PID 1 sets
  `TZ` on seat svcs only. Logs and `date` stay UTC.
- `session` — which graphical desk PID 1 runs: `sola` (River + Sola
  stack) or `omarchy` (Hyprland + Quickshell bar). Default `sola`.
  Both packs stay on disk; only one compositor is enabled. Apply of a
  session change is **confirm** (it stops the other desk). PID 1 waits
  3 s for DRM release; reboot if the incoming compositor still loses.
  Do not enable `svc:river` and `svc:hyprland` together.

## Example

```
oath schema host
oath get host:local
oath set host:local hostname=atlas
oath diff
oath apply
```

Display timezone (Sola clock; system `date` stays UTC):

```
oath set host:local timezone=MST7MDT,M3.2.0,M11.1.0
oath apply
```

Restart the graphical stack (or reboot) so seat processes pick up `TZ`.
```

Switch desk (kills the graphical session; reboot after apply until DRM wait exists):

```
oath set host:local session=omarchy
oath apply --confirm
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
