# Services

PID 1 is **oath-init** (`/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

The `base` profile:

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/lib/oath/serial-login` | enabled, `restart=always` | Root shell on the serial console (`ttyS0` / `hvc0`). With no UART it idles and logs `no serial console`. Do not disable it unless you have another way in. |
| `svc:screen` | `/lib/oath/screen-login`, `user: home` | enabled, `restart=always` | Login shell as `home` on the screen and keyboard (tty1); `sudo` for root. The screen shows it once the display driver loads, just before services start. A desk turns it off. |
| `svc:sshd` | `/bin/sshd -D -e` (OpenSSH) | enabled, `restart=always`, `keep_children` | Keys in `ssh:local`; `/etc/ssh/sshd_config` comes from `host:local`. Passwords and root login off. Restarting it keeps open sessions. |
| `svc:time` | `/bin/chronyd -d` against `0`–`3.pool.ntp.org` | enabled, `restart=always` | Keeps the clock (see [Network](network.md)); runs as `void` after start-up. |

The `desk` profile adds the desk's own (rivulet, Sola's, PipeWire's)
and two for Bluetooth: `svc:dbus-system`, the system bus
(`/run/dbus/system_bus_socket`, as `void`), and `svc:bluetooth`,
BlueZ's `bluetoothd`. Pair a device with `bluetoothctl`: `power on`,
`scan on`, then `pair`, `trust` and `connect` with its address;
`devices` lists the known ones. Headphones then show in `wpctl status`.

Add a service on the box with `oath new` and `oath apply` (below), or
at install time with an overlay (see [Catalog](catalog.md)).

## Boot and converge

At boot PID 1 mounts the floor, switch_roots into the root subvolume
`@`, mounts the btrfs top level at `/oath/run/fs`, sets the hostname from
`host:local`, loads the drivers its devices ask for, brings up `net:net0`,
refreshes `dev:*`, writes `ssh:local` keys, then starts every enabled
service in `wants` order (and the DHCP client, see
[Network](network.md)). Host, network and keys come from what was
last applied, not from pending `oath set` edits. Display and sound
drivers load after that, so `svc:sshd` is up before any GPU driver.
`oath logs` lists the modules each pass loaded.

PID 1 runs only what was applied: each service's `applied.json`, which
`oath apply` writes after the spec passes its checks. A `set` without
`apply` never reaches a running service, even across a reboot. A spec
PID 1 cannot read (a torn file) leaves the running process alone.

PID 1 reaps children and restarts them per `restart`
(`never` / `always` / `on-failure`). It stops services that are
disabled or whose applied spec changed. Each service runs in its own
cgroup (`/sys/fs/cgroup/oath/svc-<name>`): stopping it sends SIGTERM to
every process in it, waits up to 5 s, then kills what is left. With
`keep_children` (as on `svc:sshd`) only the main process is signalled.

`oath apply` on a `svc` writes `applied.json`, notifies PID 1 on
`/oath/run/init.sock`, waits for PID 1 to answer that it has
converged, then checks that actual `state` is `running` or `stopped`.
`oath undo` puts back the previous `applied.json` and does the same.

`wants` is start order: enabled wanted services start first. Disabled
or unknown wants are skipped. A cycle refuses apply. If a cycle reaches
PID 1 anyway, it skips the services in the cycle (and those waiting on
them) and starts everything else, `svc:sshd` included.

```
oath ls --kind svc
oath get svc:time
oath set svc:time enabled=false
oath apply svc:time
oath get svc:time --actual
oath undo
```

## State, restarts and logs

```
oath get svc:time --actual
oath logs svc:time          # last 50 lines; -n 200 for more
oath logs                   # PID 1's own log
```

Actual has `state`, `pid`, `restarts` (restarts this boot),
`last_exit` (`{"code": 3}` or `{"signal": 15}`), `since` (UTC) and,
while it runs from a package, `pkg` (that package and hash).
`state` is:

- `running` — the process is up.
- `restarting` — it exited and waits for its backoff.
- `stopped` — disabled, or a finished `restart=never` run.
- `failed` — it could not be started, it is in a `wants` cycle (or
  waits on one), or it died within 10 s of starting five times in a
  row; `reason` in actual says which. Restarts wait 1, 2, 4, then 8 s between
  those quick failures; a run of 10 s or more restarts at once. A
  `failed` service stays down until its applied spec or its program's
  package changes: fix it with `oath set` and `oath apply`.

A service's stdout and stderr go to `/oath/log/svc/<name>.log`, one
line each with a UTC time in front, rotated to `<name>.log.1` past
256 KiB. PID 1 writes its own messages to `/oath/log/init.log`; the
DHCP client writes to `/oath/log/net/net0.log` (`oath logs net:net0`).

## Add and remove a service

```
sudo oath new svc:tick --from-json '{"exec":["/bin/sleep","1000"],"wants":[],"restart":"always","enabled":true}'
sudo oath apply svc:tick        # starts it
sudo oath rm svc:tick
sudo oath apply svc:tick        # stops it and deletes the object
sudo oath undo                  # brings it back
```

`oath new` needs only `exec`; it writes the defaults into the object
(`restart: always`, `enabled: true`, `wants: []`), so `oath get` shows
what will run. Until `oath apply`, a new service has status `new` and
does not run; once applied it is not new, and if it cannot start (a
wrong `exec`, say) its status is `error` and its actual has the
`reason`;
`oath rm` before that deletes it at once.

`svc:sshd` changes that can cut off SSH (disabling or removing it, or
changing its `exec`, `user` or `env`) need `oath apply --confirm` and
go on probation: a new SSH session runs `sudo oath keep`, or PID 1
undoes the change after 180 s.

## Account, environment, low ports

Oath has three Unix accounts: `root`, `home` (uid 1, gid 1) and
`void` (uid and gid 65534, home `/var/empty`), the unprivileged one
that owns nothing and cannot log in. `user` is `root`, `home` or
`void`; absent means `root`. A service that needs no files of its own
and no privilege runs as `void`.

Every service gets `PATH=/bin`, `HOME`, `USER`, `LOGNAME`,
`SHELL=/bin/sh` and `XDG_RUNTIME_DIR` for its account (the `desk`
profile's Sola services set `SHELL=/bin/zsh`, so terminals start the
login shell), then
`host:local.env`, then `TZ` from `host:local.timezone` (unless `env`
already sets `TZ`), then its own `env`. The service's `env` wins on
the same name.

`bind_low_ports` lets a `home` or `void` service bind ports below 1024
(`CAP_NET_BIND_SERVICE`). Root can bind them either way. On a
service an overlay added (here `svc:web`):

```
oath set svc:web user=home bind_low_ports=true
oath set svc:web --from-json '{"env":{"WEB_ROOT":"/home/site"}}'
oath apply svc:web
```

`rtprio` (1–99) lets a service give its threads realtime priority up
to that value and renice itself to -20 (`RLIMIT_RTPRIO`,
`RLIMIT_NICE`); the `desk` profile gives PipeWire 95.

Changing `exec`, `user`, `env`, `bind_low_ports` or `rtprio` restarts
the process. So does an apply that moves the package its program comes
from: PID 1 follows `exec[0]` through `/bin` into the store
(`/oath/store/pkg/<name>/<hash>/…`), and when that realization changes
it restarts the service. Actual `pkg` names it (`{"id": "pkg:sola",
"hash": "sha256-…"}`). A service whose program is in another package
keeps running; one that was `failed` tries again with the new program.
Moving `pkg:openssh` restarts `svc:sshd`, so it needs `--confirm` and
goes on probation.

An update does not restart a service with a `seat` (the desk's, the
console's): `oath update`, and `oath apply --keep-seat`, leave it on
the program it started with, its actual `reason` says so, and it takes
the new one when it next starts (a reboot, Exit Sola and `oath desk`,
or its own exit). With `--reboot`, `oath update` restarts no service
before the reboot. A PID 1 from desk output `fdecb1ec3d68` or older
restarts the seat's services anyway, unless the update reboots.

## Reboot and power off

`sudo reboot`, `sudo poweroff` and `sudo halt` ask PID 1 to shut down,
and so does `oath set host:local power=reboot` (or `halt`) with
`oath apply --confirm`. PID 1 stops every service (SIGTERM, up to 5 s,
then SIGKILL), stops everything else, syncs, unmounts the btrfs top
level, remounts `/` read-only, then reboots or powers off. Ctrl-Alt-Del
on the console reboots the same way.

## Off-box backup

The image ships two helpers, not a service:

- `/lib/oath/backup-send host:/export/path` — snapshot `@`, mount the
  NFS export (version 4, else 3), write `<hostname>.send` (one `btrfs send` stream,
  overwritten each time) and `<hostname>.json` (generation, UTC time,
  size, sha256). It is not a second undo; `oath undo` never reads it.
- `/lib/oath/backup-daily host:/export/path` — sleep until 04:00 local
  time (`TZ`, from `host:local.timezone`; UTC when unset), run
  `backup-send`, repeat.

To run it daily, add a service in the machine's install overlay:

```json
{"objects": {"svc:backup": {
  "exec": ["/lib/oath/backup-daily", "nas:/export/backup/canto"],
  "wants": [], "restart": "always", "enabled": true}}}
```

A present package may ship
`/oath/store/pkg/<name>/live/libexec/oath-backup-quiesce` and
`oath-backup-thaw`; `backup-send` runs them around the snapshot.
Without them the copy is crash-consistent (`sync` plus btrfs CoW).

## Scheduled services

A `schedule` (R48) makes a oneshot start at times, as cron would: a
pattern (five fields, or six with seconds; `@daily`; croner's `L`, `W`
and `#`) in the machine's time zone, `host:local.timezone`, so 04:00 stays
04:00 across daylight saving.

```sh
sudo oath new svc:backup --from-json '{"exec":["/lib/oath/backup-send","nas:/export/backup/canto"],"wants":[],"restart":"never","enabled":true,"schedule":"0 4 * * *"}'
sudo oath apply svc:backup
oath get svc:backup     # state: "scheduled", next: "…04:00:00-06:00", last_run
```

Apply does not start it, and refuses a schedule without `restart:
"never"` or one croner cannot read. PID 1 starts it at each match; at
boot, once, when a match passed while the machine was down (the last run
is in `/oath/log/svc/<name>.schedule.json`); and never twice at once: a
match while the last run still goes is skipped, and `init.log` says so.
Its output goes to `oath logs svc:<name>` as any service's.

## If PID 1 panics

A panic in PID 1's main loop is logged and survived: PID 1 hands its
state to a fresh copy of itself, and every service keeps running and
stays supervised (`oath logs` shows `panic: re-executing PID 1` and
`resumed after a panic`). A third panic within a minute makes it shut
down cleanly and reboot.
