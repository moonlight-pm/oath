# dev

A hardware device. The catalog is the inventory. `/dev` is nodes from
devtmpfs, not the admin UI. There is no udevd.

**When to use:** see what the box has (`oath ls --kind dev`).

**When not:** do not run `lspci` / `udevadm` as admin. Do not `rm` in
`/dev`. Loading kernel modules is not this kind. `net:net0` is the
link; `dev:net0` is the NIC.

The `base` profile seeds `dev:vda` (root disk), `dev:net0`, and
`dev:ttyS0`. They are not removable. PID 1 refreshes their actual at
boot.

## Fields

- `present` — in inventory. `false` is refused on these objects.

Actual also has `class` (`block` / `net` / `tty`) and `node`.

Known gap: the names are fixed and QEMU-shaped. A metal machine with
`/dev/sda` still has `dev:vda` (absent in actual).

## Example

```
oath ls --kind dev
oath get dev:vda
```
