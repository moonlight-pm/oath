# apps/ — first-party programs that ship as `pkg:*`

We wrote these. They are not the OS control plane (`crates/`), not
patched upstream (`forks/`), and not borrowed prebuilts (`image/`).

Each directory is the store tree for `pkg:<name>`:

```
apps/<name>/bin/<name>  →  /oath/store/pkg/<name>/bin/<name>
```

| Path | Catalog | Notes |
|------|---------|--------|
| `hello/` | `pkg:hello` | packed into the image; removable canary |
| `fetchme/` | `pkg:fetchme` | payload the probe HTTP server wget’s |

Do not put River or Sola here. Those are `forks/`.
