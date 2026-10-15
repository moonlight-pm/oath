# Catalog

The catalog is the source of truth. It lives at `/oath`.

```
/oath/
  INDEX.md                 generated. read this first
  schema/<kind>.json       JSON Schema 2020-12
  schema/<kind>.md         prompt-sized prose
  objects/<kind>/<name>/
    desired.json
    actual.json
    meta.json              id, safety, status
    applied.json           svc only — the spec PID 1 runs
    new / remove           markers for a pending `oath new` / `oath rm`
  log/apply.jsonl          every apply (ok, error, refused) and undo
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
| `pkg` | `busybox`, `btrfs`, `oath`, `dropbear`, `openssh`, `util-linux`, `dosfstools`, `curl` | Store + `/bin` symlink farm |
| `net` | `net:net0` | The link: `dhcp` or a static address |
| `ssh` | `ssh:local` | Keys that may log in as `home` |
| `dev` | `dev:net0`, `dev:ttyS0` | Hardware inventory |
| `plan` | none until you file one | Hashed `plan.plan`; `oath build` writes a pkg tree |

`host` and `net` share a schema between desired and actual. `svc`
actual is runtime (`state`, `pid`, `restarts`); drift is desired
against `applied.json`, not against `actual.json`.

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

`dev:*` is the hardware inventory. `/dev` is nodes from devtmpfs, not
the admin UI. There is no udevd. `dev:net0` (the NIC; `net:net0` is
the link) and `dev:ttyS0` are seeded, are not removable, and get their
actual refreshed at boot. Disks are not listed yet; discovery is the
target ([devices spec](../specs/devices.md)).

PID 1 also mounts, outside the catalog: `/proc`, `/sys`, devtmpfs
`/dev` with `/dev/pts`, tmpfs `/tmp`, `/dev/shm` and `/run`, and
cgroup2 at `/sys/fs/cgroup`. `/etc/hosts` has `localhost` and the
hostname.
