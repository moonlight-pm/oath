# Catalog

The catalog is the source of truth. It lives at `/oath`.

```
/oath/
  INDEX.md                 generated. read this first
  schema/<kind>.json       JSON Schema 2020-12
  schema/<kind>.md         prompt-sized prose
  objects/<kind>/<name>/   spec (snapshots hold it; undo reads it)
    desired.json           what you asked for
    applied.json           the spec as last applied (PID 1 runs a svc's)
    meta.json              id, safety; `fixed` on a seed-fixed package
    new / remove           markers for a pending `oath new` / `oath rm`
  status/<kind>/<name>.json  what the machine observed: tmpfs, rebuilt each boot
  log/apply.jsonl          every apply (ok, error, refused), undo and keep
  log/init.log             PID 1's messages
  log/init.jsonl           PID 1's events
  log/oath.jsonl           the events of each `oath` command run as root
  log/svc/<name>.log       each service's output (`oath logs svc:<name>`)
  log/svc/<name>.schedule.json  a scheduled service's last run
  log/svc/<name>.last.json  a service's own record of its last run, shown in its actual
  log/net/net0.log         the DHCP client
  log/plan/<name>.log      a plan's last build (`oath logs plan:<name>`; `.1` is the one before)
  probation.json           an apply that can cut off SSH, until `oath keep` or an undo
  .lock                    held by whoever is writing the catalog
  store/pkg/<name>/<hash>/ package trees (apply links /bin and /lib)
  store/cell/<name>/<hash>/ cell trees (not linked into /bin or /lib)
  store/plan/<name>/<hash>/plan.plan
  store/adv/<name>/<hash>.json  a pin's advisories, when its origin serves them
  heads.json               the head `oath update`, `oath new` or `oath add` took of each package
  output.json              the release this machine runs (an install or `oath update` writes it)
  machine-id               this machine's id: made by PID 1 at a boot or take-over that lacks one
  login/password           `home`'s password hash, root's alone
  ssh/                     host keys
  run/                     sockets and mounts, not desired state
    init.sock              `oath apply` asks PID 1 to converge here
    login.sock             `svc:login`: `oath password`, `oath desk`
    seat                   who has tty1: `desk` or `console`; absent is `desk` (each boot removes it)
    fs/                    btrfs top level (subvolid=0)
```

Desired and actual are separate files. `home` may `cat` any of these,
except the SSH host's private key and `login/`, which are root's;
`oath get` is the supported way. If a kind is not under
`/oath/schema`, it does not exist. Spec, status and `INDEX.md` are
replaced atomically (temp file, fsync, rename), so a reader never sees
half of one. Logs are appended to. Past 256 KiB a log moves to
`<file>.1`, one old file kept; `apply.jsonl` stays whole, and a plan's
`.1` is its build before.

## Kinds

| Kind | Ids in the `base` profile | Role |
|------|---------------------------|------|
| `host` | `host:local` | Hostname, power (`run`, `reboot`, `halt`; `sleep` suspends to RAM, R84), `env`, `timezone`, `hosts`, `origins`, `profile`, `publishers`, `mounts`, `module_options`, `seat_hid`, `autologin` |
| `svc` | `svc:login`, `svc:screen`, `svc:serial`, `svc:sshd`, `svc:time` | PID 1's only config |
| `snap` | `snap:current`, then `snap:N` | Generations: apply and `oath undo` write them, and `oath set` refuses them |
| `pkg` | `pkg:glibc`, `pkg:bash`, `pkg:zsh`, `pkg:coreutils`, `pkg:btrfs`, `pkg:oath`, `pkg:openssh`, `pkg:curl`, … (45 in `base`, 39 present; the rest are recipes and their sources) | Store + `/bin` and `/lib` symlink farms |
| `net` | `net:net0` | The link: `dhcp`, or a static address with its `gateway` and `dns` servers; `wake`, Wake-on-LAN by magic packet (R84) |
| `ssh` | `ssh:local` | Keys that may log in as `home` |
| `dev` | `dev:net0`, `dev:disk-pci-…` (discovered) | Hardware inventory |
| `plan` | `plan:claude-code`, `plan:codex`, `plan:grok-build` | Hashed `plan.plan`; `oath build` writes a `pkg` or `cell` tree |
| `cell` | none; a profile or overlay cannot name one | One project's own software: a tree in `/oath/store/cell/`, not linked into `/bin` or `/lib`, started only by `cell run cell:<name>` (`pkg:cell`), from no origin and never published |

An id is `kind:name`. A verb whose argument is an id needs all of it:
`oath get pkg:bash`, not `oath get bash`. A name is letters, digits,
`.`, `_` and `-`, at most 64 characters, not starting with `.`. A
plan's name is narrower: lowercase letters, digits and `-`, with no
`-` first or last.

Drift is desired against `applied.json`, the spec as last applied, for
every kind; actual (`/oath/status/…`) is only what the machine observed (`svc`
actual is runtime: `state`, `pid`, `restarts`). A package or cell whose
pinned tree is gone from the store, or that a check found damaged,
drifts in `tree` too, as does a cell left unlinked because its tree was
gone at boot. `oath apply` fetches a package's tree again
([Packages](packages.md#a-damaged-package)). A cell comes from no
origin: until `sudo oath store cell:<name> <dir|file.tar>` puts its
tree back ([Cells](cells.md#cellname-one-projects-own-software)), apply
refuses it before it takes a generation, as does an `oath undo` that
would put it back, and neither changes anything. `oath store` of a tree
that hashes to the pin a package or cell is applied at records that
check, so its `tree` drift, its `error` status and its Store damage in
`INDEX.md` clear with no apply or `oath verify`; a cell left unlinked
drifts until apply links it.

`oath schema` lists the kinds, and `oath ls <kind>` (or `--kind <kind>`)
the ids of one; a word that is no kind is refused, with the kinds named
and `oath ls --origin <word>` as the hint. `oath schema <kind>` prints
the kind's prose (its fields and, where the kind has them, when to use
it, safety and examples), `---`, then its JSON Schema: the two files under
`/oath/schema/`. The prose is `crates/oath-core/schema/<kind>.md` in
the repo. The JSON Schema is generated from the kind's type in
`crates/oath-core/src/kinds.rs`, and `oath set` refuses a key it does
not list.

## Where objects come from

On the box, `oath new <id>` creates a `svc`, `pkg` or `cell` and
`oath rm <id>` removes one; both take effect at `oath apply`, and
`oath undo` of that apply reverses them. `oath new` says what that
apply does with the object: it starts a service, links a package, or
checks a cell's tree, which `cell run` then runs. `oath rm` of an object
`oath new` made, before an apply has taken it, deletes it at once and
says so (`removed: no apply had taken it`).
`host`, `net`, `ssh`, `snap`, `dev` and `plan` objects cannot be
created or removed this way, and the ten packages the `base` profile
lists as `fixed` cannot be removed.
`oath set plan:<name> --from-file <path>` files a plan, and
`oath build` creates the product's `pkg` or `cell` object, absent, if
it is missing (`--no-pkg` does not, for a product only other plans
build with). `oath update` also makes and removes packages and
services, and files plans ([Install](install.md#update-an-installed-machine)).

`oath new pkg:<name>` with no fields takes the package by name
(`oath ls --origin [word]` lists what the origins hold): its newest head
that a key in `host:local.publishers` may sign for that name, and the
same for each package it needs that the machine lacks. Each is made
present at its head's `hash` and `needs`, with the head's
`description`, `home`, `category` and `source`; its pack is fetched
into the store, and nothing is linked until `oath apply`; each head it
takes goes in `/oath/heads.json`. A need the machine lacks that has no
head refuses the command, and so does a name with no head unless the
store holds one tree of it, which is then the pin (an empty `hash`).
`oath add <name>...` makes the same objects and applies them in one
generation, recording the heads once they are linked; when that apply
fails, it leaves the machine as it was. It takes no tree from the
store, so a name with no head refuses it. Both refuse a head older than
one this machine took, for the name or a need. Both need root; what
they refuse, and how a need the machine holds at another pin is
settled, is in [Packages](packages.md#install--remove).

Before the box first boots, objects come from:

- **Seed profiles** — `crates/oath-core/seed/<profile>.json`, data that
  pkg:oath ships as `/lib/oath/seed/` and no binary carries (R72), so a
  profile's pins never change PID 1. `base` is a headless machine you
  reach over SSH; `desk` extends it with the graphical session. `oath
  seed --profile <name>` reads `/lib/oath/seed` (`OATH_SEED_DIR` for
  another directory; `cargo make` sets it to the tree's). `cargo make build
  --profile <name>` seeds the image from one. A profile has `objects`,
  the first desired document of each object; `fixed`, the packages that
  can never be removed (only the `base` profile's list is enforced); and
  `extends`, the profile it layers on. Seeding writes spec only, no
  status: each object starts applied as desired, and
  `host:local.profile` is set to the profile's name. A profile cannot
  name a `cell:`.
- **Overlays** — `{"objects": {"kind:name": {…}}}`. Each object is a
  JSON merge patch (RFC 7386) on the current desired: objects merge
  key by key, `null` deletes a key, anything else replaces. A new id
  creates the object, so its patch must stand alone as that kind's
  document (`svc:web` needs its `exec`). A `cell:` id is refused, and
  so are `extends` and `fixed`, which only a profile has. Each patched
  document must parse as its kind and pass `oath set`'s value checks
  (a host name, a POSIX TZ string, a service's `exec`). `oath set`'s
  other refusals do not apply: a key the kind does not have is kept,
  and `pkg:glibc` or an existing `dev:` object can be set absent, which
  `oath apply` then fails on. A patch to a `svc`, `host`, `net` or
  `ssh` counts as applied: PID 1 boots with it. A new object of any
  kind but `pkg` counts as applied too. A new package starts applied as
  absent, and a patch to an existing `pkg`, `plan` or `dev` stays
  desired: `oath diff` shows it and `oath apply` takes it. A patch to
  `snap:current`, which `oath set` refuses, stays desired and
  `oath diff` shows it, but no apply takes its number: the next apply
  that takes a generation, or `oath undo`, writes the machine's own
  generation over it ([Generations](generations.md)).
  `cargo make install --overlay <file.json>` checks the file against a
  catalog of the output's own profile before anything is wiped, then
  merges it into the installed catalog, after the hostname and the SSH
  keys the installer writes. The `overlay:` line in `oath get` and
  `oath diff` is something else: a package pin that is not its
  release's or head's ([Packages](packages.md)).

`oath seed` is the build- and install-time tool behind both.
It is hidden from `oath --help`:

```
oath --root ./stage/oath seed --profile base
oath --root /mnt/oath seed --overlay overlay.json
```

A fresh root gets the profile (default `base`). An existing catalog
only takes `--overlay`; `--profile` on it is refused.

## Devices

`dev:*` is the hardware inventory, discovered by PID 1 at boot and
again whenever a disk, NIC or serial port comes or goes: disks are
`dev:disk-<bus path>` (`dev:disk-pci-0000.00.1f.2-ata-1`), the NIC is
`dev:net0` (`net:net0` is its link) and other NICs `dev:net1`, …, a real
UART or USB serial port keeps its kernel name (`dev:ttyS0`,
`dev:ttyUSB0`). Desired has one field, `present`, and `oath set`
refuses to make it false. Actual has `present`, `class` (`block`, `net`
or `tty`), the `node`, bus `path`, `driver`, for a disk its `size` in
bytes and, where it has them, `model` and `removable`, and for a NIC
its `mac`, which another machine's `oath wake <mac>` sends a magic
packet for. A device that goes away loses its object. `/dev` is nodes
from devtmpfs, not the admin UI. There is no udevd; PID 1 loads the
driver a hot-plugged device asks for
([devices spec](../specs/devices.md)).

PID 1 sets device modes at boot and as nodes appear. `/dev/null`,
`/dev/zero`, `/dev/full`, `/dev/tty`, `/dev/ptmx`, `/dev/random`,
`/dev/urandom`, `/dev/net/tun` (PID 1 loads `tun` at boot), the GPUs'
render nodes (`/dev/dri/renderD*`) and NVIDIA's `/dev/nvidia*` are
0666. `/dev/kvm`, `/dev/uinput`, `/dev/snd/*`, a game controller's
`js*`, `event*` and `hidraw*`, and the vendor interfaces (`hidraw*`) of the
devices `host:local.seat_hid` lists are root:home 0660. `/dev/tty1` is
`home`'s, 0600. `/dev/dri/card*` and other input nodes stay root's;
`svc:seatd` opens those for the desk. `work` and `void` are not in
group `home`, so a service running as either reaches none of the 0660
ones. Any other module with a device name and a fixed major gets its
node at boot, root:root 0600, and the node's first open loads the
module: `/dev/uhid`, `/dev/fuse`.

PID 1 also makes these mounts, which no object describes: `/proc`,
`/sys`, devtmpfs `/dev` with `/dev/pts`, tmpfs `/tmp`, `/dev/shm`,
`/run` and `/oath/status`, cgroup2 at `/sys/fs/cgroup`, the btrfs top
level at `/oath/run/fs`, `@work` at `/work`, `@home` at `/home` when
the install made one and `host:local.mounts` does not name `/home`,
and `/lib/modules` and `/lib/firmware` from the initrd, held in
memory. `/etc/hosts` has `localhost`, the hostname and
`host:local.hosts` ([Network](network.md#names)).

## `/etc`

Do not edit these files, except `resolv.conf` on a static address
with an empty `net:net0.dns` ([Network](network.md#names)). An edit
lasts until the file's writer next runs.

| Files | Written by | When |
|-------|------------|------|
| `hostname`, `hosts`, `profile`, `zprofile`, `zshrc`, `shells`, `ssh/sshd_config` | oath, from `host:local` | each boot, and when `host:local` converges |
| `resolv.conf` | the DHCP client's hook; on a static address, oath from `net:net0.dns` (R83) | each lease and renewal that names DNS servers; on a static address, each boot and when `net:net0` converges, unless `dns` is empty ([Network](network.md#names)) |
| `passwd`, `group`, `shadow`, `nsswitch.conf`, `ssh/ssh_config` | the image | replaced by `oath update` |
| `machine-id` | PID 1, from `/oath/machine-id` | each boot, and each take-over by re-exec ([Services](services.md#a-new-pid-1-without-a-reboot)); the id is random, made at the first of these that lacks one, and `oath install` does not copy it |

There is no `fstab`, `os-release`, `localtime`, `sudoers`,
`sysctl.conf` or `crontab`. Mounts are `host:local.mounts`, the time
zone is `host:local.timezone` (a POSIX TZ string such as
`MST7MDT,M3.2.0,M11.1.0` or `UTC0`, R83: no zoneinfo ships, so
`oath set` refuses a zone name such as `America/Denver`, and a bare
`UTC`), a timed job is a service's `schedule`, and on an installed
machine the release is in `/oath/output.json`.
