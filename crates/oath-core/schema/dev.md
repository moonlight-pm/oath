# dev

A hardware device. The catalog is the inventory. `/dev` is nodes from
devtmpfs, not the admin UI. There is no udevd.

**When to use:** see what the box has (`oath ls --kind dev`).

**When not:** do not run `lspci` / `udevadm` as admin. Do not `rm` in
`/dev`. Loading kernel modules is not this kind. `net:net0` is the
link; `dev:net0` is the NIC.

PID 1 discovers the objects: at boot, and again when a disk, NIC or
serial port comes or goes (a device that is gone loses its object).
Names are stable: `disk-<bus path>` (`disk-pci-0000.00.1f.2-ata-1`),
`net0` for the NIC `net:net0` configures and `net1`, … for others, and
the kernel name of a real serial port (`ttyS0`). The seed has none.

## Fields

- `present` — in inventory. `false` is refused.

Actual also has `class` (`block` / `net` / `tty`), `node`, the bus
`path`, the `driver`, and for a disk `size` (bytes), `model` and
`removable`, for a NIC `mac`.

## Example

```
oath ls --kind dev
oath get dev:net0
```
