# apps/ — first-party programs that ship as `pkg:*`

We wrote these. They are not the OS control plane (`crates/`), not
patched upstream (`forks/`), and not borrowed prebuilts (`image/`).

Each directory is the store tree for `pkg:<name>`:

```
apps/<name>/bin/<name>  →  /oath/store/pkg/<name>/bin/<name>
```

That layout **is** the pack. Oath does not ship a recipe language;
authors produce this tree however they want. Target identity (content
hash, `/oath/store/pkg/<name>/<hash>/`, pin on `desired.hash`) is T32
([docs/specs/2026-09-03-pkg-pack-identity.md](../docs/specs/2026-09-03-pkg-pack-identity.md))
— not implemented; as-built store has no hash component.

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
