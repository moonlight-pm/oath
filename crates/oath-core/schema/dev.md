# dev

A hardware device. The catalog is the inventory. `/dev` is nodes from
devtmpfs, not the admin UI. There is no udevd.

**When to use:** see what the box has (`oath ls --kind dev`).

**When not:** do not run `lspci` / `udevadm` as admin. Do not `rm` in
`/dev`. Loading kernel modules is not this kind (yet). `net:net0` is
the link; `dev:net0` is the NIC.

v0 objects: `dev:vda` (root disk), `dev:net0`, `dev:ttyS0`,
`dev:card0` (virtio-gpu), `dev:kbd0`, `dev:mouse0`. Not removable.

## Fields

- `present` — in inventory. `false` is refused on these objects.

Actual also has `class` (`block` / `net` / `tty` / `drm` / `input`)
and `node`. `kbd0` / `mouse0` nodes come from sysfs names, not
stable event numbers.

## Example

```
oath ls --kind dev
oath get dev:vda
```
