# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 the INDEX (generated, live; also /oath/INDEX.md)
```

`--json` prints the same facts as JSON. Failures include a **hint**
pointing at the next step (`oath ls`, `oath schema <kind>`, INDEX
safety).

The catalog root is `/oath`. `--root` / `OATH_ROOT` is for tests and
for build and install tooling on the build host.

## Verbs

| Verb | Does |
|------|------|
| `oath ls` | List object ids. `--kind host` (etc.) to filter |
| `oath schema [kind]` | List kinds, or one kind's prose + JSON Schema |
| `oath get <id>` | Desired + actual + status. `--desired` / `--actual` to clip |
| `oath set <id> k=v` | Write **desired** only. Does not converge. `--from-json '<obj>'` sets several fields. The result must fit the kind's type. `oath set plan:<name> --from-file <path>` files a plan (canonical, `oath fmt -w`); `--from-file -` files the canonical form of a plan read from stdin |
| `oath new <id> k=v` | Create a `svc` or `pkg` (`--from-json` too). Runs after `apply` |
| `oath rm <id>` | Mark a `svc` or `pkg` for removal. Removed at `apply` |
| `oath diff [id]` | Drift between desired and actual (or last applied, for `svc`) |
| `oath apply [id...]` | Check, snapshot, then converge the listed ids or all drift |
| `oath keep` | Keep an apply that is on probation. Run it from a new SSH session |
| `oath undo` | Put back what the last apply changed (repeat to go further back) |
| `oath verify [pkg:…]` | Hash linked package trees again against their pins; exit 1 if one changed on disk (`-q`: only damaged ones and the count) |
| `oath log` | Apply log (JSON lines): `ok`, `error`, `refused`, `undo:N`, `keep:N`. `-n N` for the last N |
| `oath logs [id]` | A service's output (`svc:<name>`), the DHCP client's (`net:net0`), the last build of a plan (`plan:<name>`), or PID 1's (no id). `-n N` lines, default 50; `--json` gives `{id, path, lines}` |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated, netless build of a pinned plan. Does not link `/bin` |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |
| `oath mcp` | These verbs as an MCP server on stdin/stdout, for an agent ([Agents](#agents)) |

`set`, `new` and `rm` are staging. Nothing is live until `apply`, and
PID 1 never runs a spec that was not applied. `set` refuses a key the
kind does not have, or a value of the wrong type
(`oath set svc:time restart=sometimes` names the field and fails).

Writes to the catalog need root: over SSH, use `sudo oath set`,
`sudo oath apply`, and so on. Reads (`ls`, `get`, `diff`, `log`) do
not. Writers take a lock on `/oath/.lock`, so two `oath` commands never
interleave; the second waits up to 30 s.

Ids are `kind:name` — `host:local`, `svc:sshd`, `snap:current`,
`pkg:curl`, `net:net0`, `ssh:local`, `dev:net0`, `plan:hello`. Names
are letters, digits, `.`, `_` and `-` (at most 64, not starting with
`.`).

## Safety

`apply` checks every object it will touch before it changes anything.
A refused apply (bad value, `wants` cycle, broken package `needs`,
missing `--confirm`) changes nothing and takes no generation. Then it
takes a filesystem snapshot and converges.

- **mutate** — hostname, service enable, package present. Snapshot and
  go.
- **confirm** — halt, reboot, picking an old boot generation (not
  `undo`), and anything that can cut this machine off from SSH:
  disabling, removing or changing `svc:sshd`, dropping a key from
  `ssh:local`, changing `net:net0`. `oath apply` without `--confirm` exits **3** and
  explains. Do not pass `--confirm` unless the owner asked for that
  class of change.

```
oath set host:local hostname=atlas
oath diff
oath apply

oath set host:local power=reboot
oath apply              # refused (exit 3)
oath apply --confirm    # reboots
```

After a reboot apply, desired `power` is set back to `run` so the box
does not loop. The apply asks PID 1 for the reboot; PID 1 stops the
services, syncs and remounts `/` read-only first. A power change on its
own takes no generation (power is not configuration); plain
`sudo reboot` and `sudo poweroff` do the same.

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
applies; `sudo oath undo` takes it back at once.

`oath undo` puts back the spec of the objects the newest apply changed
(an apply that failed counts), converges them, and logs `undo:N`. Run
it again to undo the apply before that. It never rewinds the log, the
package store, or the SSH host keys.

## Disks

Another disk mounts through `host:local.mounts`: a path under `/mnt` or
`/home`, the filesystem by label or UUID, and mount options.

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
'mount: /home'`).

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
`si_support=1`, `nvidia_drm`'s `modeset=1`) and before `module.key=value`
on the kernel command line, so a later one wins. Like mounts, they
belong in the machine's overlay, never in a profile: another machine
with the same driver gets none of them.

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is enabled.
The screen and keyboard are **home** when `svc:screen` is enabled. SSH is **home** (uid 1, `HOME=/home`, login shell zsh). `sudo` (no
password) becomes root. The groups are `root`, `home` and `void`. The
apply log records uid and tty. `host:local.env` and `TZ` (from
`host:local.timezone`) are injected by PID 1 into services and written
to root-owned `/etc/profile` for logins, and to sshd's `SetEnv` in
`/etc/ssh/sshd_config`, so `ssh host 'command'` has them too, as of
sshd's last start. `oath apply` does not write `$HOME/.profile`.
`void` (uid 65534, home `/var/empty`) is the third account: nobody logs
in as it; sshd, dhcpcd and chronyd drop to it.

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
when `host:local.autologin` is false, and by the desk's lock screen
(Sola's); SSH (keys only) and `sudo` never ask, so it guards the screen
and keyboard, not the account: [login.md](../specs/login.md) says
exactly what it does and does not protect. Auto-login is on by default;
Sola's Settings turn it off with the password, and root can with `sudo
oath set host:local autologin=false` and `sudo oath apply host:local`.

On a desk, the screen and keyboard (tty1) are the seat, and either the
desk or the console has it. Exit Sola ends the desk on purpose
(`oath desk stop` does the same): its services are held (`state:
"held"`) and the console, a shell as `home` (or the password prompt), takes
tty1. `oath desk`, from that shell or over SSH, gives the seat back and
the desk's services start again. Every boot starts with the desk.

A machine installed before R42 gets the programs with `oath update`
(its `pkg:oath` moved first, [install.md](install.md)) but not the objects. Reboot into the update first: an older PID 1 knows no
`seat` and would start the console on tty1 under the running desk. Then
make them once:

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
14:02:07 home@canto Workspace/sola main § 
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
`ls`, `schema`, `get`, `diff`, `log`, `logs` and `verify` look;
`set_host`, `set_svc`, `set_pkg`, `set_net`, `set_ssh`, `new_svc`,
`new_pkg`, `rm`, `apply`, `undo`, `keep`, `update` (what the newest
output would move, or with `confirm` move to it), `file_plan` (a plan
from its text) and `build` run through `sudo`, as the owner's would. The same
reads are resources too: `oath://index`, `oath://schema/<kind>` and
`oath://object/<kind>/<name>`. A client that sends a `progressToken`
hears every 5 s that a long `build` or `apply` is still running. A tool's `fields` are the kind's schema,
so a field added to a kind reaches the tools with nothing else to do.
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
