# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 the INDEX (generated, live; also /oath/INDEX.md)
```

`--json` prints the same facts as JSON. Most failures include a **hint**
pointing at the next step (`oath ls`, `oath schema <kind>`, INDEX
safety); `set`, `new`, `rm`, `apply` and `undo` without root print
`io Permission denied (os error 13)` and no hint; `oath new pkg:<name>`
with no fields for a name this machine lacks prints `new changes this
machine` with the `sudo` command as the hint, and `oath add` says what
`sudo oath add` would add. The exit status is 0,
1 for a failure, 2 for a command line `oath` cannot parse, 3 for an
apply that needs `--confirm`, and 4 for a refusal printed as
`security: …`: a tree, download or signature that is not what it should
be. A reader that stops early (`oath ls --origin | head`) ends `oath`
quietly, as it ends any Unix tool.

The catalog root is `/oath`. `--root` / `OATH_ROOT` is for tests and
for build and install tooling on the build host.

Each `oath` command also prints telemetry on stderr: lines that start
`oath-tel `, one JSON object each ([Telemetry](build.md#telemetry)).
Stdout never carries it and nothing turns it off. `2>/dev/null` drops
it, and a failure's message with it unless `--json`. `oath` run as root
appends the same objects to `/oath/log/oath.jsonl`; PID 1 appends its
own (boot steps, each service's start and exit, hotplug, shutdown) to
`/oath/log/init.jsonl`. Each is rotated to `<file>.1` past 256 KiB,
keeping one old file. Nothing sends them off the machine.

## Verbs

| Verb | Does |
|------|------|
| `oath ls [kind]` | List object ids: all, or one kind (`oath ls svc`, or `--kind svc`). A word that is no kind is refused, in either form, with the kinds named and `oath ls --origin <word>` as the hint. `oath ls --origin [word]` lists what the origins hold instead (their `index.json`): each package's name, version, category and description, with `*` on those this machine has and recipes marked. A word keeps those with it in their name, category or description |
| `oath schema [kind]` | List kinds, or one kind's prose + JSON Schema |
| `oath get <id>` | Desired + actual + status. `--desired` / `--actual` to clip |
| `oath set <id> k=v` | Write **desired** only. Does not converge. `--from-json '<obj>'` sets several fields. The result must fit the kind's type. `oath set plan:<name> --from-file <path>` files a plan (canonical, `oath fmt -w`); `--from-file -` files the canonical form of a plan read from stdin |
| `oath new <id> k=v` | Create a `svc`, `pkg` or `cell` (`--from-json` too). Nothing runs until `apply`, and `new` says what the apply does: starts a `svc`, links a `pkg`, and checks a `cell`'s tree, which `cell run` then runs. `oath new pkg:<name>` with no fields makes what `oath add` would, recording its heads and refusing an older one as `oath add` does, and applies nothing; when no origin has a head of it, a tree of it already in the store is the pin |
| `oath add <name>...` | Install packages by name (`<name>` or `pkg:<name>`) from the origins: each at its newest head a key in `host:local.publishers` may sign for it, with each package it needs that this machine lacks, made and applied in one generation; an add that fails leaves the machine as it was, and `oath undo` takes back one that worked. Each head it takes is recorded in `/oath/heads.json`, as `oath update` records one, and a head older than one recorded there is refused. Nothing the machine has moves: if a need it holds at another pin would cost the package a library, nothing changes and `sudo oath update --confirm` comes first. A package already here is named and left to `oath update`, and one here with `present=false` is refused; a name with no head, such as a recipe, is refused. Without root it says what it would add and each pack's size, and changes nothing ([Packages](packages.md#install--remove)) |
| `oath rm <id>` | Mark a `svc`, `pkg` or `cell` for removal. Removed at `apply`; one no apply has taken is deleted at once, and `rm` says it was removed |
| `oath diff [id]` | Drift: desired against the spec last applied, for every kind (`field: old -> new`, or `in-sync`). A live state that differs is not drift: a mount waiting for its disk is in sync. A `pkg` or `cell` whose tree is gone from the store, or that a check found damaged, drifts in `tree` |
| `oath apply [id...]` | Check, snapshot, then converge the listed ids or all drift. `--confirm` and `--probation <secs>`: [Safety](#safety). `--keep-seat`: a running service with a `seat` keeps its program when its package moves, and takes the new one when it next starts |
| `oath keep` | Keep an apply that is on probation. Run it from a new SSH session |
| `oath undo` | Put back what the last apply changed (repeat to go further back) |
| `oath verify [id...]` | Hash linked `pkg` and `cell` trees again (all of them, or the ids given) against the hash each is linked as; exit 1 if one changed on disk or is gone from the store (`-q`: only damaged ones and the count). As `home` it only reports; `sudo oath verify` and PID 1's check at each boot record each outcome, and INDEX lists a damaged tree under **Store damage** with its repair: `sudo oath apply pkg:<name>` fetches a package's tree again, and `sudo oath store cell:<name> <dir\|file.tar>` puts a cell's back ([A damaged package](packages.md#a-damaged-package)) |
| `oath audit [pkg:<name>]` | The daily security audit's findings for what is here, when a key this machine trusts signed them; `--json` is the report ([Security audit](packages.md#security-audit)) |
| `oath reap` | What would go: versions in the store that nothing holds, filed plans nothing pins, and generations past the newest five and those since boot. Before them, a line for each version that stays says what holds it (`pkg:less sha256-ea7e1d00bd42 stays: live, pin`); `--json` gives every version and what holds it. A version stays while it is live, or a pin, a need, the release, a current plan or a kept generation names it; a tree younger than a day stays too (`--arriving <secs>`). `sudo oath reap --confirm` removes the rest, and each boot does |
| `oath log` | Apply log (JSON lines): `ok`, `error`, `refused`, `undo:N`, `keep:N`. `-n N` for the last N |
| `oath logs [id]` | A service's output (`svc:<name>`), the DHCP client's (`net:net0`), the last build of a plan (`plan:<name>`), or PID 1's (no id). `-n N` lines, default 50; `--json` gives `{id, path, lines}` |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated, netless build of a pinned plan. Does not link `/bin`. Creates the product's `pkg` or `cell` object if it is missing; `--no-pkg` does not, for a product that is only a build input to other plans |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |
| `oath store <id> <path>` | Put a tree built elsewhere (a directory, or a tar of one) into the store as `pkg:<name>` or `cell:<name>` and print its hash. Changes no spec; when that `pkg` or `cell` is linked or applied at the hash, it records the check as `sudo oath verify` would, so damage found in its tree clears with no apply ([Packages](packages.md#a-tree-built-here)) |
| `oath update` | What the newest output published for this machine, and the newest head a key it trusts signed for each package here and each the output adds, would change; `sudo oath update --confirm [--reboot]` moves to them; `--keep pkg:<name>` or `svc:<name>` (one this machine has), `--output <id>`; `--record` moves nothing and lists each package's head, and `sudo oath update --record --confirm` records each one this machine runs as the head it took (after `oath publish`). A head wins over a release's pin, so `--output` of an older output sets each package that has a head to its head, not to that output's pin. A head never goes back: one older than the head this machine took waits, and for a package the machine lacks, so does what needs it. `oath undo` puts back the pins an update moved ([Install](install.md#update-an-installed-machine)) |
| `oath publish pkg:<name>` | What it would publish: this machine's pin of the package, as its head on an origin; `--confirm` publishes it, without `sudo` ([Packages](packages.md#publishing-a-package-on-its-own-r72)) |
| `oath install [disk]` | The disks; `sudo oath install <disk>` says what it would erase and write; with `--confirm` it wipes that disk and installs this system ([Install](install.md#from-a-stick-on-the-machine-oath-install)) |
| `oath password` | Set, change or remove `home`'s password; `--status` says whether one is set, with auto-login and the seat ([below](#the-password-and-the-seat)) |
| `oath desk [stop]` | Give the seat (tty1) back to the desk, or with `stop` end the desk and give it to the console ([below](#the-password-and-the-seat)) |
| `oath wake <mac>` | Wake another machine on this LAN whose `net:net0.wake` is on (R84): Wake-on-LAN's magic packet for its MAC (`mac` in its `dev:net0`), broadcast three times. Any user may send it, and nothing answers it ([Wake-on-LAN](network.md#wake-on-lan)) |
| `oath mcp` | These verbs as an MCP server on stdin/stdout, for an agent ([Agents](#agents)) |

`set`, `new` and `rm` are staging. Nothing is live until `apply`, and
PID 1 never runs a spec that was not applied. `set` refuses a key the
kind does not have, or a value of the wrong type
(`oath set svc:time restart=sometimes` names the field and fails).
A `k=v` value that parses as JSON is that JSON (`enabled=false`;
`hostname=123` is a number, and refused); anything else is a string.
With `--from-json`, a `k=v` on the same key wins.

Writes to the catalog need root: over SSH, use `sudo oath set`,
`sudo oath apply`, and so on. Reads (`ls`, `get`, `diff`, `log`) do
not. Writers take a lock on `/oath/.lock`, so two `oath` commands never
interleave; the second waits up to 30 s.

Ids are `kind:name` — `host:local`, `svc:sshd`, `snap:current`,
`pkg:curl`, `net:net0`, `ssh:local`, `dev:net0`, `plan:hello`,
`cell:hamlet`. Names are letters, digits, `.`, `_` and `-` (at most 64,
not starting with `.`).

## Safety

`apply` checks every object it will touch before it changes anything.
A refused apply (bad value, `wants` cycle, broken package `needs`, a
cell's tree damaged or gone from the store, missing `--confirm`)
changes nothing and takes no generation. Then it takes a filesystem
snapshot and converges. A cell comes from no origin: its refusal names
the repair, `sudo oath store cell:<name> <dir|file.tar>`. Until the tree
is back, a plain `sudo oath apply` refuses; `sudo oath apply <id>` of
anything else goes on. `oath undo` checks first too: it refuses, changing
nothing, to put back a cell whose tree at that pin is damaged (exit 4)
or gone, and names the same store. The store of a tree that hashes to
the pin records the check, so the damage clears with no apply; a cell
whose tree was gone at boot waits for the apply that links it. `oath update` and `oath
install` refuse changes not applied, and name the store for a cell whose
tree is damaged or gone.

- **mutate** — hostname, service enable, package present. Snapshot and
  go.
- **confirm** — halt, reboot, sleep, and anything that can cut this
  machine off from SSH:
  disabling or removing `svc:sshd`, changing its `exec`, `user` or
  `env`, moving the package it runs from (`pkg:openssh`'s `hash`),
  dropping a key from `ssh:local`, changing `net:net0`'s `up`, `ipv4`
  or `gateway`. `oath apply` without `--confirm` exits **3** and explains. Do not pass `--confirm`
  unless the owner asked for that class of change.

```
sudo oath set host:local hostname=atlas
oath diff
sudo oath apply

sudo oath set host:local power=reboot
sudo oath apply              # refused (exit 3)
sudo oath apply --confirm    # reboots
```

After a reboot, halt or sleep apply, desired `power` is set back to
`run` so the box does not loop. The apply asks PID 1 for the reboot;
PID 1 stops the services, syncs and remounts `/` read-only first. A
power change on its own takes no generation (power is not
configuration); plain `sudo reboot` and `sudo poweroff` do the same.

`power=sleep` (R84) suspends to RAM, in the state the kernel chose,
the bracketed one in `/sys/power/mem_sleep` (`deep`, S3, or `s2idle`);
there is no hibernation.
PID 1 takes no part: the apply sleeps the machine itself and returns
when it wakes, saying how it went (`--json` gives it as `slept`):

```
sudo oath set host:local power=sleep
sudo oath apply --confirm host:local   # sleeps; on wake it prints:
# power change applied without a generation (power is not configuration)
# slept at 2026-…Z (deep), woke at 2026-…Z
# armed to wake it: <each keyboard and mouse>, the power button
# woken by: …   (only when the platform says what woke it)
```

Each USB or PS/2 keyboard and mouse wakes it (the apply arms the device,
its hubs and its controller), as does the power button; a pad, a pen or
a Bluetooth device does not. With `net:net0.wake` on, a magic packet
from `oath wake` wakes it too, and `net0` is in the armed list
([Wake-on-LAN](network.md#wake-on-lan)). With NVIDIA's driver loaded,
the apply suspends it through `/proc/driver/nvidia/suspend` first and
resumes it after, which keeps video memory (PID 1 gives `nvidia`
`NVreg_PreserveVideoMemoryAllocations=1`). On a machine that offers no
sleep the apply exits 1 with `this machine offers no sleep`; when the
kernel refuses (a device would not suspend), the hint is
`sudo dmesg | tail -n 30`.

A change that can cut off SSH goes on **probation** once applied:

```
sudo oath set net:net0 ipv4=192.168.1.20/24 gateway=192.168.1.1
sudo oath apply --confirm              # on probation for 180 s
ssh home@192.168.1.20 sudo oath keep   # a new session: kept
```

If no new session runs `sudo oath keep` in time (`--probation <secs>`,
10–3600, default 180), PID 1 undoes the apply and logs `undo:N` by
`probation`. A reboot before `keep` undoes it too. The session you
applied from cannot keep it: only a login made after the apply proves
the machine is still reachable. While a probation is open nothing else
applies, and `oath` shows it under **Probation** with its deadline;
`sudo oath undo` takes it back at once.

`oath undo` puts back the spec of the objects the newest apply changed
(an apply that failed counts), converges them, and logs `undo:N`. Run
it again to undo the apply before that. It never rewinds the log, the
package store, or the SSH host keys. Only apply and undo move
`snap:current`: `oath set` refuses it, and the
[boot menu](generations.md#boot-menu) boots an archived boot, not a
generation.

## Time zone

`host:local.timezone` is a POSIX `TZ` string:

```
sudo oath set host:local timezone=MST7MDT,M3.2.0,M11.1.0
sudo oath apply host:local
```

Empty is UTC. There is no zoneinfo database, so `set` and `apply`
refuse what is not a POSIX `TZ` string (R83): a zone name such as
`America/Denver`, and plain `UTC`, which has no offset, for `UTC0`.
Every service and every login gets `TZ` from it, unless `host:local.env`
sets `TZ`. The system clock and the logs stay UTC. A running service
keeps its old `TZ` until it restarts.

## Disks

Another disk mounts through `host:local.mounts`: a path under `/mnt` or
`/home`, its `source` (`LABEL=`, `UUID=`, `PARTLABEL=` or `PARTUUID=`),
and `options` for mount(8) `-o`, empty for the filesystem's defaults;
`bind`, `rbind`, `move` and `remount` are refused.

```
sudo oath set host:local mounts='{"/mnt/warehouse": {"source": "LABEL=warehouse", "options": "noatime,compress=zstd:1"}}'
sudo oath apply host:local
```

Apply mounts it now if the disk is plugged in, or prints that it waits
for it. PID 1 mounts it at every boot and whenever the disk appears,
and unmounts it at shutdown. `set` replaces the whole map:
`mounts='{}'` and an apply unmount it, and `oath undo` takes a change
back. `oath get host:local --actual` lists what is mounted now. Make
the filesystem first (`sfdisk`, `mkfs.btrfs -L <label>`; `blkid` shows
labels and UUIDs). A device name such as `/dev/sdb1` is refused: it
changes with the port and the boot. A machine's mounts belong in its
install overlay (`hosts/<name>.json`).

An NFS export mounts the same way, its `source` the server and path
(`host:/path`). No package is needed: Oath resolves the name, checks
that the server answers on port 2049, and mounts it with the kernel's
own client, as NFS 4.1 unless `options` name a `vers=` (`vers=3,nolock`
for a server with no NFSv4):

```
sudo oath set host:local mounts='{"/mnt/alpha": {"source": "nas:/mnt/alpha", "options": "noatime"}}'
sudo oath apply host:local
```

A server that does not answer waits; PID 1 mounts the export after boot,
once the network is up, and tries again while it is not mounted, a
minute apart and less often the longer it waits. The mount is `hard`, so
a program that reads it while the server is away waits for it, and
`soft` in `options` makes it fail instead. A server that maps every
client user to one (TrueNAS's mapall) gives every user on the machine
that one's rights: under `/home`, which only `home` and root can enter,
`work` and `void` cannot reach it; under `/mnt` they can.

Home itself can live on another volume, a btrfs subvolume of another
disk, say:

```
sudo oath set host:local mounts='{"/home": {"source": "UUID=580d861c-4b96-4017-856c-0b55ff4e58ce", "options": "subvol=@oath-home,noatime,compress=zstd:1"}}'
sudo oath apply host:local
```

The apply changes nothing yet: it prints `/home is … from the next
boot`. At the next boot PID 1 mounts it before anything reads /home,
in place of the install's `@home`, and writes sshd's keys onto it. Copy
what /home holds into the new place first (`cp -a`): the old home stays
where it was, not mounted. If the disk is missing at boot, the machine
comes up on the old home, keys written, and logs it (`oath logs | grep
'mount: /home'`). Changing or removing the entry also waits for the next
boot: an apply leaves `/home` as it is.

## Kernel module options

A machine's hardware workarounds are module parameters in
`host:local.module_options`, by module name. PID 1 passes them each
time it loads the module; one already loaded takes a change at the
next boot.

```
sudo oath set host:local module_options='{"nvidia": "NVreg_EnableGpuFirmware=0"}'
sudo oath apply host:local
```

They come after the parameters Oath always gives a module (amdgpu's
`si_support=1 cik_support=1`, `nvidia`'s
`NVreg_PreserveVideoMemoryAllocations=1` for sleep, `nvidia_drm`'s
`modeset=1 fbdev=1`) and before `module.key=value` on the kernel
command line, so a later one wins. Like mounts, they belong in the
machine's overlay, never in a profile: another machine with the same
driver gets none of them.

`sudo modprobe <name or alias>` loads a module and what it needs, with
these parameters; `sudo modprobe -r <name>` unloads one. `/proc/modules`
lists what is loaded. There is no `lsmod`, `insmod` or `rmmod`.

Any other kernel option goes on the kernel command line, the `options`
line of `loader/entries/oath.conf` on the ESP (partition 1 of the root
disk):

```
sudo mkdir -p /mnt/esp
sudo mount <esp partition> /mnt/esp
sudo vi /mnt/esp/loader/entries/oath.conf   # add to the end of the options line
sudo umount /mnt/esp
```

It takes effect at the next boot. `oath update` and `oath install` keep
what you added and write the rest of the line themselves: `console=`,
`panic=10` (a kernel panic reboots after 10 s), `amdgpu.si_support=1`,
`radeon.si_support=0`, `oath.root=` and `oath.subvol=`.

## A device's own protocol

A program on the desk (as `home`) may need to talk to a device in its
own protocol: a wireless mouse's battery, say, which the kernel does
not report. `host:local.seat_hid` lists such devices by
`vendor:product`. `grep HID_ID /sys/class/hidraw/*/device/uevent` shows
them: `0003:00003554:0000F510` is `3554:f510`.

```
sudo oath set host:local seat_hid='["3554:f510"]'
sudo oath apply host:local
```

PID 1 makes the device's vendor interfaces (`/dev/hidraw*`) root:home
0660, at once and whenever it is plugged in, whatever number it gets.
An interface that carries a keyboard, a mouse or touch stays root's, so
no program reads keys or motion; `oath apply` prints the nodes it gave,
or that the list names no vendor interface plugged in now. `set` refuses
an entry that is not `vendor:product`. The list belongs to the machine,
never a profile.

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is enabled.
The screen and keyboard are **home** when `svc:screen` is enabled. SSH
is **home** (uid 1, `HOME=/home`, login shell zsh). `sudo` (no
password) becomes root. The apply log records uid and tty.

The groups are `root`, `home`, `work` and `void`. `work` (uid 2, home
`/work`, R55) is for servers and workloads: nobody logs in as it,
`sudo` refuses it, and `/home` is closed to it. `sudo -u work <command>`
runs a command as it from `home`, and it cannot type into `home`'s
terminal, since the kernel's `TIOCSTI` needs root (R60); a service names
it in `user`. `/work` is its own subvolume, `@work`, which PID 1 makes on
the first boot that lacks it, so no generation or boot snapshot holds
its data. `void` (uid 65534, home `/var/empty`) is the fourth account:
nobody logs in as it; sshd, dhcpcd, chronyd and a desk's system bus drop
to it.

`host:local.env` and `TZ` (from `host:local.timezone`) are injected by
PID 1 into services and written to root-owned `/etc/profile` for logins,
and to sshd's `SetEnv` in `/etc/ssh/sshd_config`, so `ssh host
'command'` has them too, as of sshd's last start. `oath apply` does not
write `$HOME/.profile`.

`sudo` is Oath's own (`/lib/oath/sudo`, setuid), for `home` and root
only: `sudo [-n] [-u root|work] [--] <command>`. `-n` changes nothing,
`-u` takes only `root` or `work`, and any other option is refused, `-i`,
`-s` and `-E` among them. The command starts in a clean environment:
`PATH=/bin`, `HOME`, `USER`, `LOGNAME`, `SHELL=/bin/sh`, and the
caller's `TERM` and `TZ`, so `host:local.env` does not reach it;
`sudo env NAME=value <command>` passes one. `-u work` runs it in
`/work`, with `XDG_RUNTIME_DIR` set too.

The four accounts are fixed. There is no `su`, `passwd` or `useradd`,
and `oath update` writes `/etc/passwd`, `/etc/group` and `/etc/shadow`
again.

A program can trace or read the memory of its own descendants only
(Yama, R64), so attaching a debugger to a program already running takes
`sudo`. Landlock is on too, for a program that fences itself in with
it. The kernel's link protections are on: `fs.protected_symlinks`,
`protected_hardlinks`, `protected_fifos` and `protected_regular` are
all 1.

### The password and the seat

`home` has no password until you set one (R42, [login.md](../specs/login.md)):

```
oath password                 # asks the current one (if set), then the new one twice
oath password --status        # password: set|none, autologin: on|off, seat: desk|console
printf 'old\nnew\n' | oath password   # off a terminal: the current one (if set), then the new
```

An empty new password removes it. `svc:login` keeps it, hashed, in
`/oath/login/password` (root's alone); a wrong one is answered after a
delay that doubles each time, up to 30 s. It is asked at the console
when `host:local.autologin` is false and a password is set (with none,
the console opens the shell and asks nothing), and by the desk's lock
screen (Sola's); SSH (keys only) and `sudo` never ask, so it guards the screen
and keyboard, not the account: [login.md](../specs/login.md) says
exactly what it does and does not protect. Auto-login is on by default;
Sola's Settings turn it off with the password, and root can with `sudo
oath set host:local autologin=false` and `sudo oath apply host:local`.

On a desk, the screen and keyboard (tty1) are the seat, and either the
desk or the console has it. Exit Sola ends the desk on purpose
(`oath desk stop` does the same): its services are held (`state:
"held"`) and the console, a shell as `home` (or the password prompt), takes
tty1. `oath desk`, from that shell or over SSH, gives the seat back and
the desk's services start again. Every boot starts with the desk: PID 1
gives it the seat before any service starts, so the console's turn ends
with the boot it was taken in.

A desk that lacks `svc:login` gets the programs from `oath update`
([install.md](install.md)) but not the objects: the update lists it as
`left to the owner (oath new)`. Reboot into the update first, so PID 1 is
the output's: one that knows no `seat` starts the console on tty1 under
the running desk. Then make them once:

```
sudo oath new svc:login --from-json '{"exec":["/lib/oath/login"],"wants":[],"restart":"always","enabled":true}'
sudo oath set svc:screen enabled=true seat=console
for s in rivulet sola-river sola-shell sola-session sola-kvm; do sudo oath set svc:$s seat=desk; done
sudo oath apply
```

### The shell

`home`'s login shell is zsh (`pkg:zsh`, not removable); `/bin/sh` is
bash, and root's shell. A login zsh reads `/etc/profile` through
`/etc/zprofile`. `/etc/zshrc` sets the prompt, history in
`~/.zsh_history`, emacs keys and completion:

```
14:02:07 home@atlas src/oath main § 
```

The time, `user@host`, the last two directories of the working
directory, the git branch, and `§`. `user@host` and the branch take a
color from a hash of the hostname, so each machine keeps its own and a
new hostname gets a new one. oath writes `/etc/zprofile` and
`/etc/zshrc` with `/etc/profile`, when `host:local` converges and at
boot; put your own settings in `~/.zshrc`. `ssh home@host 'command'`
runs the command through zsh, with `host:local` env from sshd.

## Agents

An agent reaches the machine the way the owner does: over SSH as
`home`, with a key in `ssh:local`. `oath mcp` speaks the Model Context
Protocol on stdin and stdout, so an MCP client gets the verbs as tools
and the INDEX as its instructions. Nothing listens on a port; the
server lives as long as the SSH connection.

```
claude mcp add oath -- ssh -T -o BatchMode=yes home@<host> oath mcp
ssh -T home@<host> oath mcp --read-only    # only the tools that look
```

Each tool runs one `oath --json` command and returns its JSON: `index`,
`ls`, `ls_origin` (what the origins hold), `schema`, `get`, `diff`, `log`,
`logs`, `verify` and `audit` look; `set_host`, `set_svc`, `set_pkg`,
`set_cell`, `set_net`, `set_ssh`, `new_svc`, `new_pkg` (with empty
`fields`, a package by name and what it needs, not applied), `new_cell`,
`rm`, `apply`, `undo`, `keep`, `add` (packages by name with what they
need, applied at once), `update` (what the newest output and the heads
would move, or with `confirm` move them), `reap` (what would go, or with
`confirm` remove it), `file_plan` (a plan from its text) and `build` run
through `sudo`, as the owner's would. The same reads are resources too:
`oath://index`, `oath://schema/<kind>` and `oath://object/<kind>/<name>`.
A client that sends a `progressToken` hears every 5 s that a long
`build` or `apply` is still running; an `apply` that sleeps the machine
answers when it wakes. A tool's `fields` are the kind's
schema, so a field added to a kind reaches the tools with nothing else
to do.
A failed command is a tool error carrying the CLI's hint; exit 3 says
the apply needs `confirm`, which an agent passes only when the owner
asked for that class of change.

`oath log` shows what came through MCP: such entries have
`actor.via: "mcp:<client>"`, and an apply given `confirm` has
`confirm: true`. A change that can cut off SSH goes on probation as
usual, and the connection that applied it cannot keep it: a new
`ssh … oath mcp` (or `ssh home@<host> sudo oath keep`) can. The server
speaks both the current MCP revision (2026-07-28) and the
`initialize`-based ones before it.
