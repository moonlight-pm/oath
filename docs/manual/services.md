# Services

PID 1 is **oath-init** (`/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

The `base` profile:

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/lib/oath/serial-login` | enabled, `restart=always` | Root shell on the serial console (`ttyS0` / `hvc0`). With no UART it idles and logs `no serial console`. Do not disable it unless you have another way in. |
| `svc:screen` | `/lib/oath/screen-login`, `user: home` | enabled, `restart=always` | Login shell as `home` on the screen and keyboard (tty1); `sudo` for root. The screen shows it once the display driver loads. On a desk it has `seat: console`: it is `held` while the desk has the seat (tty1) and runs when the console has it. |
| `svc:login` | `/lib/oath/login` | enabled, `restart=always` | Keeps `home`'s password, hashed, and who has the seat; answers on `/oath/run/login.sock`. `oath password` and `oath desk` ask it ([Using](using.md#the-password-and-the-seat)). With `host:local.autologin` false the console checks the password with it, and while it is down waits and opens no shell. |
| `svc:sshd` | `/bin/sshd -D -e` (OpenSSH) | enabled, `restart=always`, `keep_children` | Keys in `ssh:local`; `/etc/ssh/sshd_config` comes from `host:local`. Passwords and root login off. Restarting it keeps open sessions. |
| `svc:time` | `/bin/chronyd -d` against `0`–`3.pool.ntp.org` | enabled, `restart=always` | Keeps the clock (see [Network](network.md)); runs as `void` after start-up. |

The `desk` profile adds the desk's own: `svc:rivulet`; `svc:seatd`,
which opens the GPU and input devices for it; Sola's `svc:sola-*`; the
session bus `svc:dbus-session`; and PipeWire's `svc:pipewire`,
`svc:pipewire-pulse` and `svc:wireplumber`. `svc:sola-kvm`
(`sola-kvm listen --bind 0.0.0.0:4242`) ships with `enabled: false`, so
UDP 4242 is closed until it is turned on and the machine is paired
(`sola-kvm pair <peer>` on the server). Unpaired, it logs
`not paired, not starting`, exits, and ends `failed`.

Two more are for Bluetooth: `svc:dbus-system`, the system bus
(`/run/dbus/system_bus_socket`, as `void`), and `svc:bluetooth`,
BlueZ's `bluetoothd`. Pair a device with `bluetoothctl`: `power on`,
`scan on`, then `pair`, `trust` and `connect` with its address;
`devices` lists the known ones. Headphones then show in `wpctl status`;
a game controller, keyboard or mouse on Bluetooth LE is an input device.

Over SSH, a client of the desk's session needs the runtime directory,
which an SSH login does not have:
`XDG_RUNTIME_DIR=/run/user/1 wpctl status`. Without it `wpctl` says
`Could not connect to PipeWire`.

Add a service on the box with `oath new` and `oath apply` (below), or
at install time with an overlay (see [Catalog](catalog.md)).

## Boot and converge

At boot PID 1 mounts the floor, switch_roots into the root subvolume
`@`, writes `/etc/machine-id` from `/oath/machine-id` (which it makes at
the first boot that lacks one, so each machine has its own), mounts the
btrfs top level at `/oath/run/fs`, `@home` at `/home` when the
install made one, `@work` at `/work` and a tmpfs at `/oath/status`, sets the
hostname from `host:local`, loads the drivers its devices ask for, brings up
`net:net0`, refreshes `dev:*`, mounts `host:local.mounts`, writes `ssh:local`
keys, then starts every enabled service in `wants` order (and the DHCP client,
see [Network](network.md)). Host, network and keys come from what was
last applied, not from pending `oath set` edits. The display drivers that
replace the firmware framebuffer (amdgpu, i915, xe, nouveau, radeon, NVIDIA's)
and sound load after that, so `svc:sshd` is up before them.
`oath logs` lists the modules each pass loaded, then says `ready`.

After `ready`, in the background, PID 1 runs `oath verify --quiet` and
`oath --confirm reap --boot`. What they print goes to `init.log`
(`… verified, … damaged`, `reaped …`). A package whose tree is damaged or
gone has status `error`, and `/oath/INDEX.md` lists it under `Store damage`
with its repair; the boot goes on and does not repair it.
`sudo oath apply pkg:<name>` fetches it again
([`oath verify`](using.md#verbs)).

PID 1 runs only what was applied: each service's `applied.json`, which
`oath apply` writes after the spec passes its checks. A service `oath new`
made does not run until an apply takes it, and a `set` without `apply`
never reaches a running service, even across a reboot. A spec
PID 1 cannot read (a torn file) leaves the running process alone.

PID 1 reaps children and restarts them per `restart`: `always` after
any exit, `on-failure` after a non-zero exit or a signal, `never` not
at all. PID 1 stops services that are disabled or removed, or whose
`exec`, `user`, `env`, `bind_low_ports` or `rtprio` changed. A `never`
service without a `schedule` is a oneshot: it starts once a boot. A
change to one of those five fields stops it if it still runs and does not
start it again; an apply of `enabled=false`, then one of `enabled=true`,
does.
A new spec that stops a oneshot with no exit yet this boot makes that
apply, and its `oath undo`, exit non-zero (`did not become running`):
a stopped run records no `last_exit`.
Each service runs in its own cgroup (`/sys/fs/cgroup/oath/svc-<name>`):
stopping it sends SIGTERM to every process in it, waits up to 5 s, then kills what is left. With
`keep_children` (as on `svc:sshd`, and `svc:sola-session` on a desk) only
the main process is signalled, its exit leaves the rest running, and what
they write still reaches the service's log. Applying `keep_children` or
`restart` reaches a running service at once, without restarting it.

`oath apply` on a `svc` writes `applied.json`, notifies PID 1 on
`/oath/run/init.sock`, waits for PID 1 to answer that it has
converged, then waits up to 4 s for actual `state`: `running` for an
enabled service, or `held`, `scheduled`, or the `stopped` of a finished
run (`restart=never` however it ended, `restart=on-failure` with exit 0);
`stopped` for a disabled one. If it never comes,
apply exits non-zero (`did not become running`), and its generation
stays for `oath undo`.
`oath undo` puts back the previous `applied.json` and does the same.

`wants` is start order: enabled wanted services start first. Disabled
or unknown wants are skipped. A cycle refuses apply. If a cycle reaches
PID 1 anyway, it skips the services in the cycle (and those waiting on
them) and starts everything else, `svc:sshd` included.

```
oath ls --kind svc
oath get svc:time
sudo oath set svc:time enabled=false
sudo oath apply svc:time
oath get svc:time --actual
sudo oath undo
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
- `stopped` — disabled, a finished `restart=never` run, or a
  `restart=on-failure` run that exited 0. A finished run stays down
  through every later converge (a changed `exec` included) until the next
  boot, an apply of `enabled=false` and then `enabled=true`, its `seat`
  going to the other owner and back, or an applied `restart` that starts
  it after that exit, such as `always`.
- `scheduled` — it has a `schedule` and waits for its next match; actual has
  `next` and `last_run` ([Scheduled services](#scheduled-services)).
- `held` — it is enabled and has a `seat` (the desk's, the console's), and
  the other owner has the seat; `reason` says which.
- `failed` — it could not be started, it is in a `wants` cycle (or
  waits on one), or it died within 10 s of starting five times in a
  row; `reason` in actual says which. Restarts wait 1, 2, 4, then 8 s between
  those quick failures; a run of 10 s or more restarts at once. A
  `failed` service stays down until its applied spec or its program's
  package changes, its `seat` changes hands, or the next boot: fix it
  with `oath set` and `oath apply`.

A service's stdout and stderr go to `/oath/log/svc/<name>.log`, one
line each with a UTC time in front, rotated to `<name>.log.1` past
256 KiB. PID 1 writes its own messages to `/oath/log/init.log`; the
DHCP client writes to `/oath/log/net/net0.log` (`oath logs net:net0`).
PID 1's events (`/oath/log/init.jsonl`) and those of each `oath` command
run as root (`oath.jsonl`) rotate the same way, to `.1`.
The kernel's own messages are in `dmesg`, for this boot only. No log
keeps them across a reboot.

## Add and remove a service

```
sudo oath new svc:tick --from-json '{"exec":["/bin/sleep","1000"],"wants":[],"restart":"always","enabled":true}'
sudo oath apply svc:tick        # starts it
sudo oath rm svc:tick
sudo oath apply svc:tick        # stops it and deletes the object
sudo oath undo                  # brings it back
```

`exec` is the argv; it may be empty only while `enabled` is false. Give
`exec[0]` as an absolute path: a bare name is found on `PATH=/bin` and
runs, but actual has no `pkg` and the service does not follow its
package. A program the machine lacks comes in its package from the
origin: `oath ls --origin <word>` finds the package, and
`sudo oath add <name>` installs it. A service applied before its program
is here is `failed` (`cannot start <exec[0]>: No such file or
directory (os error 2)`) and, with an absolute `exec[0]`, starts once
`oath add` applies the package.
`oath new` needs only `exec`; it writes the defaults into the object
(`restart: always`, `enabled: true`, `wants: []`), so `oath get` shows
what will run. Until `oath apply`, a new service has status `new` and
does not run; once applied it is not new, and if it cannot start (a
wrong `exec`, say) its status is `error` and its actual has the
`reason`. While its status is `new`, `oath rm` deletes it at once and
says so (`svc:tick removed: no apply had taken it`).

`svc:sshd` changes that can cut off SSH (disabling or removing it, or
changing its `exec`, `user` or `env`) need `oath apply --confirm` and
go on probation: a new SSH session runs `sudo oath keep`, or PID 1
undoes the change after 180 s.

PID 1 sets `kernel.modprobe`, `net.ipv4.ping_group_range` and
`fs.protected_symlinks`, `protected_hardlinks`, `protected_fifos` and
`protected_regular`. There is no `/etc/sysctl.conf`; another sysctl is a
oneshot, which runs at every boot:

```
sudo oath new svc:sysctl --from-json '{"exec":["/bin/sysctl","-w","vm.swappiness=10"],"wants":[],"restart":"never","enabled":true}'
sudo oath apply svc:sysctl
```

## Restart a service

There is no restart verb. `sudo kill <pid>` (`pid` is in
`oath get svc:<name> --actual`) ends it, and PID 1 starts it again per
`restart`. A `failed` service, or a finished one (`restart=never`, or
`restart=on-failure` after exit 0), runs again after an apply of
`enabled=false` and an apply of `enabled=true`;
each takes a generation.

## Account, environment, low ports

Oath has four Unix accounts: `root`, `home` (uid 1, gid 1), `work` (uid and
gid 2, home `/work`), for servers and workloads, which `sudo` refuses and which
cannot log in, and `void` (uid and gid 65534, home `/var/empty`), the unprivileged
one that owns nothing and cannot log in. `user` is `root`, `home`, `work` or
`void`; absent means `root`. A service that keeps data and needs no `sudo` runs
as `work`; one that needs no files of its own and no privilege runs as `void`.

Every service gets `PATH=/bin`, `HOME`, `USER`, `LOGNAME`,
`SHELL=/bin/sh` and `XDG_RUNTIME_DIR` for its account (the `desk`
profile's Sola services set `SHELL=/bin/zsh`, so terminals start the
login shell), then
`host:local.env`, then `TZ` from `host:local.timezone` (unless `env`
already sets `TZ`), then its own `env`. The service's `env` wins on
the same name. The `desk` profile's `host:local.env` sets
`LANG=en_US.UTF-8`, `SSL_CERT_FILE` (curl's CA bundle), the D-Bus bus
addresses and `PKG_CONFIG_PATH`.

`bind_low_ports` lets a service of any account but root bind ports below 1024
(`CAP_NET_BIND_SERVICE`). Root can bind them either way. On a
service an overlay added (here `svc:web`):

```
sudo oath set svc:web user=home bind_low_ports=true
sudo oath set svc:web --from-json '{"env":{"WEB_ROOT":"/home/site"}}'
sudo oath apply svc:web
```

`rtprio` (1–99) lets a service give its threads realtime priority up
to that value and renice itself to -20 (`RLIMIT_RTPRIO`,
`RLIMIT_NICE`); the `desk` profile gives PipeWire 95.

Changing `exec`, `user`, `env`, `bind_low_ports` or `rtprio` restarts
the process. So does an apply that moves the package its program comes
from: PID 1 follows `exec[0]` through `/bin` into the store
(`/oath/store/pkg/<name>/<hash>/…`), and when that realization changes
it restarts the service (`oath logs` shows
`svc:sshd: /bin/sshd is now pkg:openssh sha256-…; restarting`). Actual
`pkg` names it (`{"id": "pkg:sola", "hash": "sha256-…"}`). A service whose program is in another package
keeps running; one that was `failed` tries again with the new program.
Moving `pkg:openssh` restarts `svc:sshd`, so it needs `--confirm` and
goes on probation.

`seat` is `desk` or `console`: whose turn on the seat (tty1, its screen
and keyboard) the service runs in. PID 1 runs it only in that owner's
turn and holds it for the other's (`state: "held"`). Absent, it runs
either way. `oath desk stop` gives the seat to the console until
`oath desk` gives it back or the machine boots: every boot starts in the
desk's turn
([The password and the seat](using.md#the-password-and-the-seat)).

An update does not restart a service with a `seat` (the desk's, the
console's): `oath update`, and `oath apply --keep-seat`, leave it on
the program it started with, its actual `reason` says so, and it takes
the new one when it next starts (a reboot, Exit Sola and `oath desk`,
or its own exit). With `--reboot`, `oath update` restarts no service
before the reboot, unless sshd's program moved: that apply goes on probation
and the services whose programs moved restart at once, the seat's excepted
([install.md](install.md)).

## Reboot and power off

`sudo reboot`, `sudo poweroff` and `sudo halt` ask PID 1 to shut down,
and so does `sudo oath set host:local power=reboot` (or `halt`, which
powers off) with `sudo oath apply --confirm`. PID 1 stops the services
in reverse `wants` order, each before what it wants, one layer at a
time (SIGTERM, up to 5 s, then SIGKILL; `oath logs` shows
`shutdown: stopping …` for each layer). Then it stops everything else,
syncs, unmounts `host:local.mounts`, `/home`, `/work` and the btrfs top
level, remounts `/` read-only, and reboots or powers off; `sudo halt`
alone halts and leaves the power on. Ctrl-Alt-Del on the console
reboots the same way.

`reboot`, `halt` and `poweroff` take one flag, `-f` (`--force`): it does
not ask PID 1, only syncs and calls the kernel, so no service stops and
`/` is not remounted.

## Off-box backup

The image ships two helpers, not a service:

- `/lib/oath/backup-send host:/export/path` — mount the
  NFS export (version 4, else 3), snapshot `@` as a generation (`snap:<n>`,
  reason `backup`), write `<hostname>.send` (one `btrfs send` stream,
  overwritten each time) and `<hostname>.json` (generation, UTC time,
  size, sha256). A service named `svc:backup` (below) shows the last run's
  `generation`, `time`, `dest`, `checksum` and `size` in its actual. The
  copy is not a second undo; `oath undo` never reads it.
- `/lib/oath/backup-daily host:/export/path` — sleep until 04:00 local
  time (`TZ`, from `host:local.timezone`; UTC when unset), run
  `backup-send`, repeat.

To run it daily, add a service in the machine's install overlay:

```json
{"objects": {"svc:backup": {
  "exec": ["/lib/oath/backup-daily", "nas:/export/backup"],
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
04:00 across daylight saving. The zone is a POSIX TZ string: `oath set`
refuses a zone name such as `America/Denver` (R83). Give a zone with
daylight saving its rules (`MST7MDT,M3.2.0,M11.1.0`): `MST7MDT` alone
schedules in UTC.
A `next` already planned keeps its time when `host:local.timezone`
changes. PID 1 takes the new zone at its next converge, but a `next`
planned in that same converge is usually still in the old zone. Each
`next` it plans after that (after a run, for a new or changed pattern,
at a boot) is in the new one. Clearing the zone reaches PID 1 only at
a boot.

```sh
sudo oath new svc:backup --from-json '{"exec":["/lib/oath/backup-send","nas:/export/backup"],"wants":[],"restart":"never","enabled":true,"schedule":"0 4 * * *"}'
sudo oath apply svc:backup
oath get svc:backup     # state: "scheduled", next: "…04:00:00-06:00", last_run
```

Apply does not start it. `oath new`, `oath set` and `oath apply` refuse
a schedule without `restart: "never"` or one croner cannot read. PID 1
starts it at each match; at boot, or when an apply enables it or changes
its pattern, once, when a match passed since the last run (kept in
`/oath/log/svc/<name>.schedule.json`; with no last run, it waits for its
first match); and never twice at once: a match while the last run still
goes is skipped, and `init.log` says so.
Its output goes to `oath logs svc:<name>` as any service's.

## If PID 1 panics

A panic in PID 1's main loop is logged and survived: PID 1 hands its
state to a fresh copy of itself, and every service keeps running and
stays supervised (`oath logs` shows `panic: re-executing PID 1` and
`resumed after a panic`). A third panic within a minute makes it shut
down cleanly and reboot.

## A new PID 1 without a reboot

`sudo oath update --confirm` (no `--reboot`) has PID 1 re-execute into
the output's binary the same way, every service running (R73,
[install.md](install.md)). `oath logs` shows `upgrade: … rehearsing`,
`upgrade: re-executing PID 1 into …` and `upgrade: this binary took
over`; `/run/oath/init.json` says which binary PID 1 runs (`exe`) and
how it started (`how`: `boot`, `panic` or `upgrade`). The PID 1 that
takes over, after an update or a panic, writes `/etc/machine-id` from
`/oath/machine-id` as boot does, and makes that id when the machine has
none (`made this machine's id`). A binary with the wrong hash, or one
that cannot read PID 1's state, is refused (`upgrade: not into …`), and
PID 1 goes on as it was.
