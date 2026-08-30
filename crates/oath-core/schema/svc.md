# svc

A process supervised by PID 1. PID 1 is not an object; it is the engine
that converges `svc:*`. There is no unit file.

**When to use:** start, stop, or change a supervised process.

**When not:** do not hand-write init dialects. Do not add a service by
editing `/etc`.

## Fields

- `exec` — argv, `exec[0]` absolute.
- `wants` — other `svc` ids that should be up first. PID 1 starts them
  in that order. Cycles refuse `oath apply`. Disabled/unknown wants
  are skipped (ordering, not Requires). `svc:hold` wants `svc:serial`.
- `restart` — `never` | `always` | `on-failure`.
- `enabled` — if false, the process is not running.

Safety: `mutate`. Apply writes desired, then notifies PID 1.

## Example

```
oath ls --kind svc
oath get svc:serial
oath set svc:serial enabled=true
oath apply svc:serial
```

The appliance ships `svc:serial` (serial root shell), `svc:hold`
(`/bin/sleep 86400000`, for start/stop tests), `svc:sshd`,
`svc:seatd`, and `svc:river` (patched River on `dev:card0`; wants
seatd). Do not disable serial unless you have another console.

```
oath set svc:hold enabled=false
oath apply svc:hold
```
