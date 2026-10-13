# Services and PID 1

PID 1 is ours. It mounts the floor, brings up the network and SSH, and
supervises one process per `svc:*` object. `svc` objects are its only
configuration: there are no unit files, no init scripts, and no
systemd. PID 1 itself is not an object; it is the engine that
converges `svc`.

**Status:** partly built. Supervision, ordering, restart policy and
accounts are built. Since 2026-10-13 PID 1 runs only applied specs and
a `wants` cycle no longer stops everything. Logs, exit codes, backoff, a
failed state, cgroups, an orderly shutdown and crash handling are the
target (phase 4).

Replaces: the `svc` parts of T1 and T7, and the account parts of T31.

## Object (built)

```json
{
  "exec": ["/bin/dropbear", "-F", "-E", "-s", "-w", "-r", "/oath/ssh/host_ed25519", "-p", "22"],
  "wants": [],
  "restart": "always",
  "enabled": true
}
```

- `exec` — argv with an absolute program path.
- `wants` — services that should start first. Ordering only: an
  unknown or disabled want is ignored. A cycle refuses apply (logged
  `refused`).
- `restart` — `never`, `always` or `on-failure`.
- `enabled` — when false, the process is not running.
- `user` — `root` (the default when absent) or `home`. Oath has these
  two accounts and no others.
- `env` — extra environment for this process. It wins over
  `host:local.env`.
- `bind_low_ports` — a `home` process keeps `CAP_NET_BIND_SERVICE`.
  Ignored for root.

Actual: `state` (`stopped`, `running`, `failed`), `pid`, `restarts`.
`applied.json` is the spec PID 1 runs: `oath apply` writes it after the
document passes its checks, and `oath undo` puts back the previous one.
Desired is what the owner is staging; PID 1 never reads it while an
`applied.json` exists.

The base profile seeds `svc:serial` (root shell on the serial console),
`svc:sshd`, and `svc:hold` (a canary that wants `svc:serial`).

## Spawn environment (built)

Every service gets `PATH=/bin`, `HOME`, `USER`, `LOGNAME`, `SHELL=/bin/sh`,
`XDG_RUNTIME_DIR=/run/user/<uid>`, then `host:local.env` plus `TZ` from
`host:local.timezone`, then its own `env`. The child calls `setsid`,
drops to its account, and execs.

## Boot sequence (built)

1. Mount `/proc`, `/sys`, `/dev`, `/dev/pts`; tmpfs on `/tmp`,
   `/dev/shm` and `/run`; cgroup2 on `/sys/fs/cgroup`; bring up `lo`.
2. In install mode (`oath.install` on the command line), start the
   installer ramdisk instead (see boot-and-install.md).
3. Load the early modules, mount the root subvolume, and enter it.
4. Mount the btrfs top level at `/oath/run/fs` so generations are
   siblings of `@`.
5. Seed only if `/oath/objects` is missing; regenerate a missing
   `INDEX.md`. Take the catalog lock for the boot-time writes.
6. Converge `host:local`, `net:net0` and `ssh:local` from their applied
   state (`actual.json`), not from pending desired; probe `dev:*`.
   Release the lock.
7. Start enabled services in `wants` order.
8. Load the deferred modules (display and sound), then listen on
   `/oath/run/init.sock`. `oath apply` writes to that socket to ask
   PID 1 to converge services again.

The main loop reaps children and restarts them according to `restart`.
On converge, PID 1 reads each `applied.json` (falling back to desired
only when there is none). A running service whose applied spec changed,
or that is disabled or gone, is stopped with SIGTERM and replaced. A
spec PID 1 cannot read leaves the running process alone and is logged.
Services in a `wants` cycle, and services waiting on one, are skipped
and recorded `stopped`; every other service, `svc:sshd` included, still
starts. PID 1 does not recreate the status file of an object `oath rm`
deleted.

## Known defects (from the 2026-10-13 review)

- Service output goes to the console. There is no per-service log, no
  exit code in actual, and `restarts` is not a real counter.
- Restarts have a fixed 200 ms delay and no backoff, so a crashing
  service spins.
- No cgroup per service; stopping a service leaves its children.
- No shutdown sequence: reboot does not stop services or sync in order.
  Plain `reboot` (busybox, which signals PID 1) does nothing; only
  `oath` `power=reboot` reboots. This is the one gap case left in
  `check/cases.toml`.
- The root is entered with `chroot`, not `switch_root`, so the initrd
  stays pinned in memory.
- If PID 1 panics or errors at boot it runs `/bin/sh` and then sleeps
  forever, and metal has no `panic=` to reboot.

## Target

- **Logs.** A ring buffer per service in PID 1, persisted under
  `/oath/log/svc/<name>`, read with `oath logs svc:<name>`.
- **Exit status.** Actual records the last exit code or signal, a true
  restart count, and the time of the last start.
- **Backoff and failed.** Exponential restart backoff with a cap. After
  N fast failures the service is `failed` and stays down until the
  next apply touches it.
- **Cgroups.** One cgroup per service under `/sys/fs/cgroup/oath/`.
  Stop kills the whole group.
- **Shutdown.** `power=reboot` or `halt` stops services in reverse
  `wants` order with a timeout, syncs, unmounts, then reboots.
- **switch_root** into the real root; the initrd is freed.
- **Crash path.** A panic hook logs, syncs, and reboots; metal kernels
  boot with `panic=10`. A rescue shell is still offered on the serial
  console.
- **Isolate bad specs, with a reason.** Today a skipped service is
  `stopped` and the reason is only in the console log. It will be
  `failed` with the reason in actual.
- **DHCP as a service.** The lease client runs as a supervised service
  instead of inside `net` converge (network-and-ssh.md).

## Courage test

Disable `svc:hold`, apply, see it stopped; enable it, apply, see it
running; undo restores the previous state. `cargo make check` also
covers a torn `desired.json` (the service keeps running) and a cycle at
boot (sshd still starts). Target additions: a crashing service backs off
and reaches `failed`; `oath logs` shows its last lines; plain `reboot`
shuts down in order.
