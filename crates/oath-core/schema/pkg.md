# pkg

A package. Bits live under `/oath/store/pkg/<name>/`. `/bin` is a
symlink farm, not an unpack target. There is no `apt` and no
`oath install`.

**When to use:** install or remove a package (`present`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not fetch; v0 payloads are already on the image. Busybox
applets are not packages yet.

## Fields

- `present` — `true` links `store/.../bin/*` into `/bin`. `false`
  removes **this object’s** links. Store stays so re-install needs no
  network.

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

The appliance ships `pkg:hello` (absent until you apply). `/bin/hello`
prints `hello`.
