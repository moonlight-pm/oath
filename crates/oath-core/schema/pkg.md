# pkg

A package. Bits live under `/oath/store/pkg/<name>/`. `/bin` is a
symlink farm, not an unpack target. There is no `apt` and no
`oath install`.

**When to use:** install or remove a package (`present`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not fetch; v0 payloads are already on the image. Do not set
`present=false` on `pkg:busybox`, `pkg:btrfs`, or `pkg:oath`.

## Fields

- `present` — `true` links `store/.../bin/*` into `/bin`. `false`
  removes **this object’s** links. Store stays so re-install needs no
  network.

Actual also has `links` (basenames in `/bin`) and `removable`. If
`removable` is false, `present=false` is refused (not `--confirm`).

Safety: `mutate`. Apply snapshots, then converges links.

## Example

```
oath ls --kind pkg
oath get pkg:hello
oath set pkg:hello present=true
oath apply
hello
oath set pkg:hello present=false
oath apply
oath undo
```

The appliance ships `pkg:busybox`, `pkg:btrfs`, and `pkg:oath`
already present (not removable), and `pkg:hello` absent until you
apply. `/bin/hello` prints `hello`. Busybox applets are one package.
