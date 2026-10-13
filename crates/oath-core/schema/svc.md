# svc

A process supervised by PID 1. PID 1 is not an object; it is the engine
that converges `svc:*`. There is no unit file.

**When to use:** start, stop, or change a supervised process.

**When not:** do not hand-write init dialects. Do not add a service by
editing `/etc`. Create one with `oath new svc:<name>` (every required
field: `exec`, `restart`, `enabled`), remove one with `oath rm`; both
take effect at `oath apply`.

## Fields

- `exec` — argv, `exec[0]` absolute.
- `wants` — other `svc` ids that should be up first. PID 1 starts them
  in that order. Cycles refuse `oath apply`. Disabled or unknown wants
  are skipped (ordering, not Requires). `svc:hold` wants `svc:serial`.
- `restart` — `never` | `always` | `on-failure`. `never` is a oneshot:
  PID 1 starts it when enabled and does not start it again after it
  exits, until `enabled` goes false then true (or the next boot).
- `enabled` — if false, the process is not running.
- `user` — `root` or `home`. Oath has those two Unix accounts and no
  others (`home` is uid 1, gid 1). Absent means `root`.
- `env` — string map, empty when absent. PID 1 puts these in the
  process environment after `host:local.env` and `TZ`. The same name
  on this object wins.
- `bind_low_ports` — boolean, false when absent. When true, a service
  that runs as `home` keeps `CAP_NET_BIND_SERVICE` and may bind ports
  below 1024. Root can already bind them.
- `keep_children` — boolean, false when absent. Each service runs in
  its own cgroup, and stopping it stops the whole process tree. When
  true, stop and restart signal only the main process and its children
  keep running (`svc:sshd` sets this so login sessions survive).

Changing `exec`, `user`, `env`, or `bind_low_ports` restarts the
process on the next converge.

Actual is runtime: `state` (`running`, `restarting`, `stopped`, or
`failed`), `pid`, `restarts` (how many times PID 1 restarted it this
boot), `last_exit` (`{"code": n}` or `{"signal": n}`), and `since`.
A process that exits is restarted after 1, 2, 4, 8 s when it keeps
dying within 10 s of starting; after five quick failures in a row it
is `failed` and stays down until its applied spec changes. Its output
(stdout and stderr) is in `/oath/log/svc/<name>.log`: `oath logs
svc:<name>`. Drift is desired against `applied.json` (the last applied
desired), not against actual.

Safety: `mutate`, except `svc:sshd`: disabling or removing it, or
changing its `exec`, `user` or `env`, needs `--confirm`. Apply checks
the document, writes `applied.json`, then notifies PID 1. PID 1 runs
only `applied.json`: a `set` without `apply` never reaches a running
service, and a file it cannot read leaves the process running. A
`wants` cycle that reaches PID 1 skips only the services in it.

## Example

```
oath ls --kind svc
oath get svc:hold
oath set svc:hold enabled=false
oath apply svc:hold
oath get svc:hold --actual
oath logs svc:hold
oath undo
```

The `base` profile ships `svc:serial` (root shell on the serial
console), `svc:sshd` (dropbear; login as `home`), and `svc:hold`
(`/bin/sleep 86400000`, a harmless process for start/stop). Do not
disable `svc:serial` unless you have another way in.
