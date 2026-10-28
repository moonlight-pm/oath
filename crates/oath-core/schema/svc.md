# svc

A process supervised by PID 1. PID 1 is not an object; it is the engine
that converges `svc:*`. There is no unit file.

**When to use:** start, stop, or change a supervised process.

**When not:** do not hand-write init dialects. Do not add a service by
editing `/etc`. Create one with `oath new svc:<name> exec=…` (only
`exec` is required; `restart` defaults to `always`, `enabled` to
`true`, `wants` to `[]`, and the object records them), remove one with
`oath rm`; both take effect at `oath apply`.

## Fields

- `exec` — argv, `exec[0]` absolute.
- `wants` — other `svc` ids that should be up first. PID 1 starts them
  in that order. Cycles refuse `oath apply`. Disabled or unknown wants
  are skipped (ordering, not Requires).
- `restart` — `never` | `always` | `on-failure`. `never` is a oneshot:
  PID 1 starts it when enabled and does not start it again after it
  exits, until `enabled` goes false then true (or the next boot).
- `enabled` — if false, the process is not running.
- `user` — `root`, `home` or `void`. Oath has those three Unix
  accounts and no others (`home` is uid 1, gid 1; `void` is 65534, home
  `/var/empty`, for a service that needs nothing). Absent means `root`.
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
- `rtprio` — 1 to 99, absent by default. The process may give its
  threads realtime priority up to this (`RLIMIT_RTPRIO`) and renice
  itself to -20 (`RLIMIT_NICE`); `desk` gives PipeWire 95.

Changing `exec`, `user`, `env`, `bind_low_ports` or `rtprio` restarts
the process on the next converge. So does an apply that moves the package
its program comes from (`exec[0]` through `/bin` into
`/oath/store/pkg/<name>/<hash>/`): a service follows its package.

Actual is runtime: `state` (`running`, `restarting`, `stopped`, or
`failed`), `pid`, `restarts` (how many times PID 1 restarted it this
boot), `last_exit` (`{"code": n}` or `{"signal": n}`), `since`, `pkg`
(`{id, hash}`: the package realization its program runs from), and
for `failed` the `reason`.
A process that exits is restarted after 1, 2, 4, 8 s when it keeps
dying within 10 s of starting; after five quick failures in a row it
is `failed` and stays down until its applied spec, or its program's
package, changes. Its output
(stdout and stderr) is in `/oath/log/svc/<name>.log`: `oath logs
svc:<name>`. Drift is desired against `applied.json` (the last applied
desired), not against actual.

Safety: `mutate`, except `svc:sshd`: disabling or removing it,
changing its `exec`, `user` or `env`, or moving the package it runs
from (`pkg:dropbear`), needs `--confirm` and goes on probation (a new
SSH session runs `sudo oath keep`, or PID 1 undoes it after 180 s).
Apply checks the document, writes `applied.json`, then notifies PID
1. PID 1 runs only `applied.json`: a `set` without `apply` never
reaches a running service, and a file it cannot read leaves the process running. A
`wants` cycle that reaches PID 1 skips only the services in it (they
are `failed`, `reason` names the cycle). At shutdown a service stops
before the services it wants.

## Example

```
oath ls --kind svc
oath get svc:time --actual
oath logs svc:time
oath new svc:tick --from-json '{"exec":["/bin/sleep","86400000"]}'
oath apply svc:tick
oath set svc:tick enabled=false
oath apply svc:tick
oath undo
```

The `base` profile ships `svc:serial` (root shell on the serial
console; it idles, and says so in its log, on a machine with no UART),
`svc:sshd` (dropbear; login as `home`), and `svc:time` (chronyd
against pool.ntp.org). Do not disable `svc:serial` unless you have
another way in.
