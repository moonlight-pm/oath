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
  probation.json           an apply that can cut off SSH, until `oath keep`
  log/init.log             PID 1's messages
  log/svc/<name>.log       each service's output (`oath logs svc:<name>`)
  log/net/net0.log         the DHCP client
  .lock                    held by whoever is writing the catalog
  store/pkg/<name>/<hash>/ package trees (apply links /bin)
  store/plan/<name>/<hash>/plan.plan
  ssh/                     host keys
  run/                     sockets and mounts, not desired state
    init.sock              `oath apply` asks PID 1 to converge here
    fs/                    btrfs top level (subvolid=0)
```

Desired and actual are separate files. You may `cat` any of these;
`oath get` is the supported way. If a kind is not under
`/oath/schema`, it does not exist. Every file is replaced atomically
(temp file, fsync, rename), so a reader never sees half of one.

## Kinds

| Kind | Ids in the `base` profile | Role |
|------|---------------------------|------|
| `host` | `host:local` | Hostname, power, `env`, `timezone` |
| `svc` | `svc:serial`, `svc:sshd`, `svc:time` | PID 1's only config |
| `snap` | `snap:current`, then `snap:N` | Generations |
| `pkg` | `glibc`, `bash`, `coreutils`, `btrfs`, `oath`, `dropbear`, `curl`, … (33 in `base`) | Store + `/bin` and `/lib` symlink farms |
| `net` | `net:net0` | The link: `dhcp` or a static address |
| `ssh` | `ssh:local` | Keys that may log in as `home` |
| `dev` | `dev:net0`, `dev:disk-pci-…` (discovered) | Hardware inventory |
| `plan` | none until you file one | Hashed `plan.plan`; `oath build` writes a pkg tree |

Drift is desired against `applied.json`, the spec as last applied, for
every kind; actual (`/oath/status/…`) is only what the machine observed (`svc`
actual is runtime: `state`, `pid`, `restarts`).

`oath schema <kind>` on the box gives fields, safety, and examples.
That text is the same as `crates/oath-core/schema/` in the repo.

## Where objects come from

On the box, `oath new <id>` creates a `svc` or `pkg` and `oath rm <id>`
removes one; both take effect at `oath apply`, and `oath undo` of that
apply reverses them. `host`, `net`, `ssh`, `snap` and `dev` objects,
and the base packages, cannot be created or removed. Before the box
first boots, objects come from:

- **Seed profiles** — `crates/oath-core/seed/<profile>.json`, compiled
  into `oath`. `base` (the only one today) is a headless machine you
  reach over SSH. A profile can `extend` another. `cargo make build
  --profile <name>` seeds the image from one; a profile lists the first
  desired document of each object, the packages that can never be
  removed (`fixed`), and any first actual that differs from the
  default.
- **Overlays** — `{"objects": {"kind:name": {…}}}`. Each object is a
  JSON merge patch (RFC 7386) on the current desired: objects merge
  key by key, `null` deletes a key, anything else replaces. A new id
  creates the object. Each patched document must pass the same checks
  as `oath set`. An overlay counts as applied: PID 1 boots with it.
  `cargo make install --overlay <file.json>` merges one into the
  installed catalog, after the hostname and the SSH keys the installer
  writes.
- **Plans** — `oath set plan:<name> --from-file` files a plan, and
  `oath build` creates the product's `pkg` object if it is missing.

`oath seed` is the build- and install-time tool behind the first two.
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
UART keeps its kernel name (`dev:ttyS0`). Actual has the `node`, bus
`path`, `driver`, and for a disk its `size`, `model` and `removable`.
A device that goes away loses its object. `/dev` is nodes from
devtmpfs, not the admin UI. There is no udevd; PID 1 loads the driver
a hot-plugged device asks for ([devices spec](../specs/devices.md)).

PID 1 also mounts, outside the catalog: `/proc`, `/sys`, devtmpfs
`/dev` with `/dev/pts`, tmpfs `/tmp`, `/dev/shm` and `/run`, and
cgroup2 at `/sys/fs/cgroup`. `/etc/hosts` has `localhost`, the
hostname and `host:local.hosts` ([Network](network.md#names)).
