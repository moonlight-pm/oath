# apps/ — first-party programs that ship as `pkg:*`

We wrote these. They are not the OS control plane (`crates/`), not
patched upstream (`forks/`), and not borrowed prebuilts (`image/`).

Each directory is the store tree for `pkg:<name>`:

```
apps/<name>/bin/<name>  →  /oath/store/pkg/<name>/<hash>/bin/<name>
```

That layout **is** the pack. Oath does not ship a recipe language;
authors produce this tree however they want. Realization id is
SHA-256 of `oath-tree-v1`; store is `/oath/store/pkg/<name>/<hash>/`;
pin on `desired.hash` (T32). Host cache:

```
cargo make store --name hello --from apps/hello --tar
```

writes `.cache/oath/store/pkg/hello/<hash>/` and an optional `.tar`.

Optional, next to `bin/`:

```
apps/<name>/libexec/oath-backup-quiesce
apps/<name>/libexec/oath-backup-thaw
```

Present packs with those executables are frozen across a generation
snapshot (T33). No hook is the default (crash-consistent).

| Path | Catalog | Notes |
|------|---------|--------|
| `hello/` | `pkg:hello` | packed into the image; removable canary |
| `fetchme/` | `pkg:fetchme` | payload the probe HTTP server wget’s |

Do not put River or Sola here. Those are `forks/`.
