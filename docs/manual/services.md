# Services

PID 1 is **oath-init** (`/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

The `base` profile:

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/lib/oath/serial-login` | enabled, `restart=always` | Root shell on the serial console (`ttyS0` / `hvc0`). Do not disable it unless you have another way in. |
| `svc:sshd` | `/bin/dropbear -F -E -s -w -r /oath/ssh/host_ed25519 -p 22` | enabled, `restart=always` | Keys in `ssh:local`. Passwords off (`-s`), root login off (`-w`). |
| `svc:hold` | `/bin/sleep 86400000` | enabled, `restart=always` | Harmless process for start/stop. Wants `svc:serial`. |

Add a service on the box with `oath new` and `oath apply` (below), or
at install time with an overlay (see [Catalog](catalog.md)).

## Boot and converge

At boot PID 1 mounts the floor, mounts the root subvolume `@` and the
btrfs top level at `/oath/run/fs`, sets the hostname from
`host:local`, loads the early kernel modules, brings up `net:net0`,
refreshes `dev:*`, writes `ssh:local` keys, then starts every enabled
service in `wants` order. Host, network and keys come from what was
last applied, not from pending `oath set` edits. The KMS and sound modules load after that,
so `svc:sshd` is up before any GPU driver.

PID 1 runs only what was applied: each service's `applied.json`, which
`oath apply` writes after the spec passes its checks. A `set` without
`apply` never reaches a running service, even across a reboot. A spec
PID 1 cannot read (a torn file) leaves the running process alone.

PID 1 reaps children and restarts them per `restart`
(`never` / `always` / `on-failure`). It stops services that are
disabled or whose applied spec changed.

`oath apply` on a `svc` writes `applied.json`, notifies PID 1 on
`/oath/run/init.sock`, and waits until actual `state` is `running` or
`stopped`. `oath undo` puts back the previous `applied.json` and does
the same.

`wants` is start order: enabled wanted services start first. Disabled
or unknown wants are skipped. A cycle refuses apply. If a cycle reaches
PID 1 anyway, it skips the services in the cycle (and those waiting on
them) and starts everything else, `svc:sshd` included.

```
oath ls --kind svc
oath get svc:hold
oath set svc:hold enabled=false
oath apply svc:hold
oath get svc:hold --actual
oath undo
```

Actual `state` is `running`, `stopped`, or `failed` (the spawn itself
failed).

## Add and remove a service

```
sudo oath new svc:tick --from-json '{"exec":["/bin/sleep","1000"],"wants":[],"restart":"always","enabled":true}'
sudo oath apply svc:tick        # starts it
sudo oath rm svc:tick
sudo oath apply svc:tick        # stops it and deletes the object
sudo oath undo                  # brings it back
```

`oath new` needs every required field (`exec`, `restart`, `enabled`).
Until `oath apply`, a new service has status `new` and does not run;
`oath rm` before that deletes it at once.

`svc:sshd` changes that can cut off SSH (disabling or removing it, or
changing its `exec`, `user` or `env`) need `oath apply --confirm`.

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

Changing `user`, `env`, or `bind_low_ports` restarts the process.

## Off-box backup

The image ships two helpers, not a service:

- `/lib/oath/backup-send host:/export/path` — snapshot `@`, mount the
  NFS export, write `<hostname>.send` (one `btrfs send` stream,
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

## Known gaps

- No per-service log, no exit code in actual, no real restart count,
  no backoff (a crashing `restart=always` service respawns every
  200 ms), no cgroup per service.
- A service skipped for a `wants` cycle shows `stopped`; the reason is
  only on the console.
- No ordered shutdown: plain `reboot` does nothing, and a catalog
  reboot or halt does not stop services first.
- If PID 1 itself fails, it runs `/bin/sh` on the console and then
  sleeps forever. Metal boots without `panic=`, so a kernel panic
  does not reboot.
