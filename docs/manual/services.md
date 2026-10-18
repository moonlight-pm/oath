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
| `svc:sshd` | `/bin/dropbear -F -E -s -w -r /oath/ssh/host_ed25519 -p 22` | enabled, `restart=always`, `keep_children` | Keys in `ssh:local`. Passwords off (`-s`), root login off (`-w`). Restarting it keeps open sessions. |
| `svc:time` | `/bin/chronyd -d` against `0`–`3.pool.ntp.org` | enabled, `restart=always` | Keeps the clock (see [Network](network.md)). |

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
`last_exit` (`{"code": 3}` or `{"signal": 15}`) and `since` (UTC).
`state` is:

- `running` — the process is up.
- `restarting` — it exited and waits for its backoff.
- `stopped` — disabled, or a finished `restart=never` run.
- `failed` — it could not be started, it is in a `wants` cycle (or
  waits on one), or it died within 10 s of starting five times in a
  row; `reason` in actual says which. Restarts wait 1, 2, 4, then 8 s between
  those quick failures; a run of 10 s or more restarts at once. A
  `failed` service stays down until its applied spec changes: fix it
  with `oath set` and `oath apply`.

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

Oath has two Unix accounts: `root` and `home` (uid 1, gid 1). `user`
is `root` or `home`; absent means `root`.

Every service gets `PATH=/bin`, `HOME`, `USER`, `LOGNAME`,
`SHELL=/bin/sh` and `XDG_RUNTIME_DIR` for its account, then
`host:local.env`, then `TZ` from `host:local.timezone` (unless `env`
already sets `TZ`), then its own `env`. The service's `env` wins on
the same name.

`bind_low_ports` lets a `home` service bind ports below 1024
(`CAP_NET_BIND_SERVICE`). Root can bind them either way. On a
service an overlay added (here `svc:web`):

```
oath set svc:web user=home bind_low_ports=true
oath set svc:web --from-json '{"env":{"WEB_ROOT":"/home/site"}}'
oath apply svc:web
```

Changing `exec`, `user`, `env`, or `bind_low_ports` restarts the
process.

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

## If PID 1 panics

A panic in PID 1's main loop is logged and survived: PID 1 hands its
state to a fresh copy of itself, and every service keeps running and
stays supervised (`oath logs` shows `panic: re-executing PID 1` and
`resumed after a panic`). A third panic within a minute makes it shut
down cleanly and reboot.
