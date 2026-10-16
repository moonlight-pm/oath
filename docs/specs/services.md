# Services and PID 1

PID 1 is ours. It mounts the floor, brings up the network and SSH, and
supervises one process per `svc:*` object. `svc` objects are its only
configuration: there are no unit files, no init scripts, and no
systemd. PID 1 itself is not an object; it is the engine that
converges `svc`.

**Status:** built, except the target at the end (surviving its own
panic). Since 2026-10-16 a service PID 1 skips or gives up on says
why in actual, and shutdown runs in reverse `wants` order. Since
2026-10-13 PID 1 runs only applied specs, isolates a `wants` cycle,
logs every service, restarts with backoff into a `failed` state, gives
each service a cgroup, shuts down in order, and switch_roots into the
disk. Since 2026-10-14 it loads modules by modalias (devices.md).

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
- `keep_children` — stop and restart signal only the main process; its
  children keep running. Default false: the whole process tree stops.
  `svc:sshd` sets it so login sessions survive an sshd restart.

Actual: `state` (`running`, `restarting`, `stopped`, `failed`), `pid`,
`restarts` (restarts this boot), `last_exit` (`{"code": n}` or
`{"signal": n}`), `since`, and for `failed` a `reason` (in a wants
cycle and with whom, waiting on one, quick failures, or the spawn
error).
`applied.json` is the spec PID 1 runs: `oath apply` writes it after the
document passes its checks, and `oath undo` puts back the previous one.
Desired is what the owner is staging; PID 1 never reads it while an
`applied.json` exists.

The base profile seeds `svc:serial` (root shell on the serial console;
it idles on a machine with no UART), `svc:sshd`, and `svc:time`.

## Spawn environment (built)

Every service gets `PATH=/bin`, `HOME`, `USER`, `LOGNAME`, `SHELL=/bin/sh`,
`XDG_RUNTIME_DIR=/run/user/<uid>`, then `host:local.env` plus `TZ` from
`host:local.timezone`, then its own `env`. stdin is `/dev/null`; stdout
and stderr are one pipe to PID 1. The child calls `setsid`, joins its
cgroup (`/sys/fs/cgroup/oath/<kind>-<name>`), drops to its account, and
execs.

## Boot sequence (built)

1. Mount `/proc`, `/sys`, `/dev`, `/dev/pts`; tmpfs on `/tmp`,
   `/dev/shm` and `/run`; cgroup2 on `/sys/fs/cgroup`; bring up `lo`.
   PID 1 catches the shutdown signals and turns Ctrl-Alt-Del into one.
2. In install mode (`oath.install` on the command line), start the
   installer ramdisk instead (see boot-and-install.md).
3. Coldplug modules by modalias, holding back display and sound
   (devices.md), and mount the root subvolume at `/newroot`.
   **switch_root:** move `/dev`, `/proc`, `/sys` and `/run` into it,
   move it over `/` and chroot there. No `/newroot` remains, and each of
   `/tmp`, `/dev/shm`, `/run`, `/dev/pts` is mounted once. The initramfs
   stays in memory: its modules and firmware are bind-mounted into the
   new root for the deferred pass.
4. Mount the btrfs top level at `/oath/run/fs` so generations are
   siblings of `@`.
5. Seed only if `/oath/objects` is missing; regenerate a missing
   `INDEX.md`. Take the catalog lock for the boot-time writes.
6. Converge `host:local`, `net:net0` and `ssh:local` from their applied
   state (`actual.json`), not from pending desired; probe `dev:*`.
   Release the lock.
7. Listen on `/oath/run/init.sock`, then start enabled services in
   `wants` order (and the DHCP client, below).
8. Load the deferred modules (display and sound), then enter the main
   loop.

## Supervision (built)

The main loop handles shutdown requests, reaps children, moves their
output into logs, starts restarts whose backoff is over, and answers
`/oath/run/init.sock`. `oath apply` writes `converge` there and waits
for the reply, which PID 1 sends after converging, so apply returns
after the change has happened.

On converge, PID 1 reads each `applied.json` (falling back to desired
only when there is none). A running service whose applied spec changed,
or that is disabled or gone, is stopped and replaced. A spec PID 1
cannot read leaves the running process alone and is logged. Services in
a `wants` cycle, and services waiting on one, are skipped and recorded
`failed` with the reason (`wants cycle with svc:b`, `waits on a wants
cycle among svc:a, svc:b`); every other service, `svc:sshd` included,
still starts. PID
1 does not recreate the status file of an object `oath rm` deleted.

- **Logs.** Each service's stdout and stderr go to
  `/oath/log/svc/<name>.log`, one UTC-stamped line at a time, rotated to
  `.1` past 256 KiB (one old file kept). PID 1's own messages go to
  `/oath/log/init.log`. `oath logs svc:<name>` and `oath logs` read
  them (`-n` lines, default 50).
- **Exit and restart.** Actual records the last exit code or signal.
  `restart` decides whether it runs again. A run of 10 s or more
  restarts at once; quick failures wait 1, 2, 4 then 8 s. After five
  quick failures in a row the service is `failed` and stays down until
  its applied spec changes. A spawn error is also `failed`.
- **Cgroups.** One cgroup per service under `/sys/fs/cgroup/oath/`.
  Stop sends SIGTERM to every process in it, waits up to 5 s, then
  `cgroup.kill`s the rest; an exit cleans up leftover children the same
  way. With `keep_children`, only the main process is signalled.
- **Shutdown.** SIGTERM or SIGINT (busybox `reboot`, Ctrl-Alt-Del)
  reboots; SIGUSR1 (`halt`) halts; SIGUSR2 (`poweroff`) powers off. An
  `oath` `power=reboot` or `halt` apply sends the same signal. PID 1
  stops the services in reverse `wants` order: in layers, each a set
  that nothing still running wants, so a service stops before the ones
  it wants (TERM to the layer, up to 5 s, KILL; `shutdown: stopping …`
  in `init.log`). Then it signals everything else, syncs, unmounts
  `/oath/run/fs`, remounts `/` read-only, and calls the kernel.
- **DHCP client.** When the applied `net:net0` is up and `dhcp`, PID 1
  runs `udhcpc -f -i net0` as an internal service (id `net:net0`, not a
  catalog `svc`, log `/oath/log/net/net0.log`) with the same restart
  and backoff, so the lease renews. See network-and-ssh.md.
- **Crash path.** A panic is logged (`init.log`, console) and panics
  the kernel; `panic=10` on the kernel command line (QEMU and metal)
  reboots it. If boot itself fails, PID 1 runs a root shell on the
  console and respawns it, reaping every child.

## Target

- **Survive a panic.** A supervisor panic reboots the machine; keeping
  services up through it is not designed yet.

## Courage test

`cargo make check` covers it: make, disable and enable `svc:hold`; undo; a
torn `desired.json` (the service keeps running); a cycle at boot (sshd
still starts; the cycle's members are `failed` with the reason);
shutdown in reverse `wants` order; a crashing service that backs off to `failed` with exit
code 3; `oath logs`; stopping a service that forked; an sshd restart
that keeps this SSH session; the DHCP client; switch_root; and plain
`reboot`.
