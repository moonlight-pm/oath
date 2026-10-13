# snap

Filesystem generations. `oath apply` snapshots the live btrfs root
before it converges. `oath undo` restores the catalog from the last
apply's snapshot.

**When to use:** look at generations; undo the last apply. Do not pick
an old generation as the boot default unless the owner asked
(`--confirm`).

**When not:** QEMU/qcow2 snapshots on the hypervisor are not this kind.

## Objects

- `snap:current` — `{ "generation": N }`, the last snapshot id.
  Changing it except via `oath undo` is **confirm**.
- `snap:N` — read-only record of generation N (`generation`, `parent`,
  `time`, `reason`). Do not `set` these.

Generation 0 is the image as first booted (no `snap:0` file until
something snapshots).

Known gaps (scheduled): a failed apply is not logged, so `oath undo`
after a failure reverts the previous good apply. Undo also restores
`/oath/log` and `/oath/store`, not only the objects.

## Example

```
oath get snap:current
oath ls --kind snap
oath undo
```
