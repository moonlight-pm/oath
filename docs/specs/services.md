# Services and PID 1

PID 1 is ours. It mounts the floor, brings up the network and SSH, and
supervises one process per `svc:*` object. `svc` objects are its only
configuration: there are no unit files, no init scripts, and no
systemd. PID 1 itself is not an object; it is the engine that
converges `svc`.

**Status:** built. Since 2026-10-16 a service PID 1 skips or gives up
on says why in actual, shutdown runs in reverse `wants` order, and a
panic in PID 1's main loop is survived with every service running. Since
2026-10-13 PID 1 runs only applied specs, isolates a `wants` cycle,
logs every service, restarts with backoff into a `failed` state, gives
each service a cgroup, shuts down in order, and switch_roots into the
disk. Since 2026-10-14 it loads modules by modalias (devices.md).

Replaces: the `svc` parts of T1 and T7, and the account parts of T31.

## Object (built)

```json
{
  "exec": ["/bin/sshd", "-D", "-e"],
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
- `user` — `root` (the default when absent), `home`, or `void` (R36:
  uid 65534, home `/var/empty`, for a service that needs nothing). Oath
  has these three accounts and no others.
- `env` — extra environment for this process. It wins over
  `host:local.env`.
- `bind_low_ports` — a `home` or `void` process keeps
  `CAP_NET_BIND_SERVICE`. Ignored for root.
- `keep_children` — stop and restart signal only the main process; its
  children keep running. Default false: the whole process tree stops.
  `svc:sshd` sets it so login sessions survive an sshd restart.
- `rtprio` — 1 to 99: PID 1 sets the process's `RLIMIT_RTPRIO` to it
  and `RLIMIT_NICE` to 40 before it drops to the account, so the
  process may give its threads that realtime priority and renice to
  -20 itself. Absent: neither. `desk` gives `svc:pipewire` and
  `svc:pipewire-pulse` 95; PipeWire's module-rt then makes its data
  threads `SCHED_FIFO` (88) without RTKit or a system bus.
- `seat` — `desk` or `console` (R42, [login.md](login.md)): PID 1 runs
  the service only while that owner has the seat (`/oath/run/seat`, the
  desk's at boot) and holds it otherwise, actual `state: "held"` with a
  `reason`; apply counts held as converged. Absent: it runs either way.
- `schedule` (is, 2026-10-31, R48) — a cron pattern (croner's: five
  fields or six with seconds, `@daily`, `L`, `W`, `#`) in the machine's
  time zone: PID 1 sets its own `TZ` from the applied
  `host:local.timezone` (or `host:local.env`'s `TZ`). Only with
  `restart: never`. Converge plans it instead of starting it: the next
  match, or now when a match passed since its last run
  (`/oath/log/svc/<name>.schedule.json`, so a run owed across a reboot
  runs at boot; a new schedule waits for its first match). The main loop
  starts it at that time, records the run and plans the next; a match
  while it still runs is skipped and logged. Actual between runs is
  `state: "scheduled"` with `next` and `last_run`; apply counts it as
  converged. A resumed PID 1 keeps each next time.

Actual: `state` (`running`, `restarting`, `stopped`, `failed`), `pid`,
`restarts` (restarts this boot), `last_exit` (`{"code": n}` or
`{"signal": n}`), `since`, `pkg` (the package realization its program
runs from), and for `failed` a `reason` (in a wants cycle and with
whom, waiting on one, quick failures, or the spawn error).

A service follows its package (is, 2026-10-20). PID 1 records, at each
start, the realization `exec[0]` resolves to through `/bin`
(`/oath/store/pkg/<name>/<hash>/…`). Apply tells PID 1 to converge once
every package in it has moved; a running `svc:*` whose program now
resolves to another realization is stopped and started again, and a
`failed` one tries again. Nothing else restarts: a service whose
libraries moved keeps running until it restarts, and PID 1's own DHCP
client does not follow `pkg:dhcpcd`. Because `svc:sshd` follows
`pkg:openssh`, moving that package's `hash` needs `--confirm` and goes
on probation, as any change that can cut off SSH does.

An update leaves the seat's services running (is, 2026-10-30). `oath
update`'s apply, and `oath apply --keep-seat`, ask PID 1 to `converge
keep-seat` on `init.sock`: a running service with a `seat` whose
program moved keeps the one it runs, its actual `reason` says so, and a
later converge leaves it too unless its package moves again; it takes
the new program when it next starts (a reboot, the seat changing hands,
its own exit). Restarting the desk ends a person's session: on canto on
2026-10-29 an update restarted rivulet under the desk, and every
Wayland client exited. `oath update --reboot` asks PID 1 for nothing,
since the boot starts every service on its new program, unless sshd's
program moved, when a new session must reach the new sshd to keep the
apply. A PID 1 from desk output `fdecb1ec3d68` or older takes `converge
keep-seat` as a plain converge.

`applied.json` is the spec PID 1 runs: `oath apply` writes it after the
document passes its checks, and `oath undo` puts back the previous one.
Desired is what the owner is staging; PID 1 never reads it while an
`applied.json` exists.

The base profile seeds `svc:serial` (root shell on the serial console;
it idles on a machine with no UART), `svc:screen` (a login shell as
`home` on tty1, the screen and keyboard; R15), `svc:sshd`, and
`svc:time`. The `desk` profile (R20) adds `svc:seatd` (root) and
`svc:rivulet` (as `home`, wanting `svc:seatd`) and sets `svc:screen`
`enabled: false`: the compositor owns the display. Services start
before the deferred display modules load; rivulet waits up to 10 s for
a KMS device, and a restart covers a GPU that takes longer.

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
   spec (`applied.json`), not from pending desired; discover `dev:*`
   and mount the `host:local.mounts` whose disk is there.
   Release the lock.
7. Listen on `/oath/run/init.sock`, then start enabled services in
   `wants` order (and the DHCP client, below).
8. Load the deferred modules (display and sound), then enter the main
   loop.

## Supervision (built)

The main loop handles shutdown requests, reaps children, moves their
output into logs, starts restarts whose backoff is over, and answers
`/oath/run/init.sock`. `oath apply` writes `converge` there (an
update's apply, or `--keep-seat`, `converge keep-seat`) and waits for
the reply, which PID 1 sends after converging, so apply returns after
the change has happened.

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
  its applied spec or its program's package changes, which starts a
  fresh count. A spawn error is also `failed`.
- **Cgroups.** One cgroup per service under `/sys/fs/cgroup/oath/`.
  Stop sends SIGTERM to every process in it, waits up to 5 s, then
  `cgroup.kill`s the rest; an exit cleans up leftover children the same
  way. With `keep_children`, only the main process is signalled.
- **Shutdown.** SIGTERM or SIGINT (`reboot`, Ctrl-Alt-Del) reboots;
  SIGUSR1 (`halt`) halts; SIGUSR2 (`poweroff`) powers off. `reboot`,
  `halt` and `poweroff` are links to PID 1's binary (R18) that send it
  that signal; with `-f` they sync and call the kernel themselves. An
  `oath` `power=reboot` or `halt` apply sends the same signal. PID 1
  stops the services in reverse `wants` order: in layers, each a set
  that nothing still running wants, so a service stops before the ones
  it wants (TERM to the layer, up to 5 s, KILL; `shutdown: stopping …`
  in `init.log`). Then it signals everything else, syncs, unmounts
  `host:local.mounts` and `/oath/run/fs`, remounts `/` read-only, and
  calls the kernel.
- **DHCP client.** When the applied `net:net0` is up and `dhcp`, PID 1
  runs `dhcpcd -B … net0` as an internal service (id `net:net0`, not a
  catalog `svc`, log `/oath/log/net/net0.log`) with the same restart
  and backoff, so the lease renews. See network-and-ssh.md.
- **Crash path.** A panic is logged (`init.log`, console). In the main
  loop it is caught: PID 1 writes its supervisor state (every child's
  pid, spec, run time and log pipe; pending restarts; `failed` specs and
  their reasons; restart history; running helpers) and the fd numbers of
  its log pipes, `init.sock` and the uevent socket, all left open, to
  `/run/oath/init-resume.json`, and execs its own binary again
  (`/proc/self/exe`, `argv[1] = oath-resume`). Services are untouched:
  they stay PID 1's children. The new image skips boot, takes the state
  and fds back, logs `resumed after a panic (n in the last minute)`, and
  runs the loop. A third panic within a minute is not resumed: PID 1
  shuts down cleanly and reboots. A panic during boot, before the loop,
  still panics the kernel (`panic=10` reboots it); if boot fails with
  an error, PID 1 runs a root shell on the console and respawns it,
  reaping every child. `sudo oath panic-init` (hidden; root only, by
  `SO_PEERCRED`) makes PID 1 panic for the harness.

## Target

None open short of the desk (phase 8).

## Courage test

`cargo make check` covers it: make, disable and enable `svc:hold`; undo; a
torn `desired.json` (the service keeps running); a cycle at boot (sshd
still starts; the cycle's members are `failed` with the reason);
shutdown in reverse `wants` order; a crashing service that backs off to `failed` with exit
code 3; `oath logs`; stopping a service that forked; an sshd restart
that keeps this SSH session; the DHCP client; switch_root; plain
`reboot`; and a PID 1 panic that a running service does not notice.
