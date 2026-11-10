# host

The machine itself. There is exactly one object: `host:local`.

**When to use:** change the hostname; reboot or halt; set the
environment every service gets; set the local timezone; name other
machines the network's DNS does not; say where packs come from; mount
disks; give a kernel module this machine's options; turn auto-login on
or off.

**When not:** do not invent extra host objects. Network and packages are
not this kind. Do not add a Unix user kind — the accounts are always
`root`, `home`, `work` (R55: servers and workloads, no `sudo`) and
`void` (R36: owns nothing).

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
  rewritten at every boot and every apply of `host:local`, so do not
  edit it. A name is letters, digits, `-` and `.`; `localhost` and this
  hostname are refused (`/etc/hosts` has them). `set` replaces the whole
  map. Empty in the `base` profile. Prefer the network's DNS when it can
  serve the name: it covers every machine at once.
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
  output, only packages' heads (R72).
- `publishers` — the keys whose signed outputs and heads `oath update`
  takes: `ssh-ed25519 AAAA… name` lines, one per publisher. A line may
  start with `names="sola,sola-*"` (a trailing `*` matches any rest):
  that key signs only the heads of those packages (`pkg/<name>.json`,
  R72), never an output or the audit's report; `names="*"` signs any
  package's head. A key with no list signs all three. The `base` profile
  lists buccina's. A person adds a key; an agent does not.
- `mounts` — filesystems to mount: a path under `/mnt` or `/home`, or
  `/home` itself (home on another volume, R45), to `{source, options}`.
  `source` is `LABEL=`, `UUID=`, `PARTLABEL=` or `PARTUUID=` a value; a
  device name (`/dev/sdb1`) moves between boots, so it is refused.
  `options` are mount(8) `-o` options, empty for the filesystem's
  defaults. PID 1 mounts each at boot and when its disk appears; apply
  mounts, remounts and unmounts. A source not plugged in waits and is not
  an error. `/home` itself changes only at boot, before sshd and the
  desk. Actual lists what is mounted now. `set` replaces the whole map.
- `module_options` — kernel module parameters by module name (`nvidia`:
  `NVreg_EnableGpuFirmware=0`): one machine's hardware workarounds, never
  a default (R34). PID 1 passes them when it loads the module, after its
  own defaults and before `module.key=value` on the kernel command line.
  A module already loaded takes a change at the next boot.
- `seat_hid` — USB `vendor:product` ids (`3554:f510`) whose vendor
  interfaces belong to the seat (R71). PID 1 makes a listed device's
  hidraw node `root:home` 0660 when its interface has a vendor-defined
  collection and no keyboard, mouse or digitizer one: a program as `home`
  speaks the device's own protocol (a mouse's battery) and never reads
  its keys or motion. On every plug, and at once on apply.
- `autologin` — boolean, true when absent (R42). True: the console
  (`svc:screen`) opens a shell as `home` without asking. False, with a
  password set (`oath password`): it asks for that password first.
  `svc:login` sets it when given the password; `oath password --status`
  shows it.

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

A HID device's vendor interface for the desk (a mouse's battery, R71):

```
oath set host:local seat_hid='["3554:f510"]'
oath apply host:local
```

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
