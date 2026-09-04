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
    applied.json           `svc` only — last applied desired
  log/apply.jsonl
  store/pkg/<name>/        package trees (not desired state; apply links /bin)
  run/                     sockets, mounts — not desired state
    init.sock
    fs/                    btrfs top-level (subvolid=0)
```

Desired and actual are separate files. Do not mix is and will-be.
You may `cat` any of these; `oath get` is the supported way.

If a kind is not under `/oath/schema`, it does not exist.

## Kinds (v0)

| Kind | Ids | Role |
|------|-----|------|
| `host` | `host:local` only | Hostname, power, `env`, `timezone` (display; system clock UTC). |
| `svc` | `svc:serial`, `svc:hold`, `svc:sshd`, `svc:seatd`, `svc:river`, `svc:sola-bus`, `svc:sola-call`, `svc:sola-river`, `svc:sola-shell`, `svc:sola-session`, `svc:sola-kvm`, `svc:backup` | PID 1’s only config |
| `snap` | `snap:current`, `snap:N` | Generations |
| `pkg` | `busybox`, `btrfs`, `oath`, `dropbear`, `glibc`, `river`, `sola`, `grok`, `git`, `curl`, `hello`, `fetchme` | Store + `/bin` symlink farm |
| `net` | `net:net0` | Static IPv4 or `dhcp` |
| `ssh` | `ssh:local` | Home authorized keys (root SSH denied) |
| `dev` | `vda`, `net0`, `ttyS0`, `card0`, `kbd0`, `mouse0` | Hardware inventory |

`host` and `snap:current` share schema between desired and actual.
`svc` actual is runtime (`state`, `pid`, `restarts`); drift is desired
vs `applied.json`, not vs `actual.json`.

See `oath schema <kind>` on the box for fields, safety, and examples.
The Markdown there is the same text shipped in this repo under
`crates/oath-core/schema/`.
