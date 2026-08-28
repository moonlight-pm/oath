# svc

A process supervised by PID 1. PID 1 is not an object; it is the engine
that converges `svc:*`. There is no unit file.

**When to use:** start, stop, or change a supervised process.

**When not:** do not hand-write init dialects. Do not add a service by
editing `/etc`.

## Fields

- `exec` — argv, `exec[0]` absolute.
- `wants` — other `svc` ids that should be up first. No cycles.
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

The appliance ships `svc:serial` (serial root shell) and `svc:hold`
(`/bin/sleep 86400000`, for start/stop tests). Do not disable serial
unless you have another console.

```
oath set svc:hold enabled=false
oath apply svc:hold
```
