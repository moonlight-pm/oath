# snap

Filesystem generations. `oath apply` snapshots the live btrfs root
before it converges (after checking everything; a refused apply takes
no generation). `oath undo` puts back, from the newest not-undone
apply's snapshot, the spec of the objects that apply changed, and
converges them. Repeat to step further back.

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

Every apply is logged: `ok`, `error` (converging failed after the
snapshot; undo reverts it), or `refused` (no generation). Undo never
rewinds `/oath/log`, `/oath/store` or the SSH host keys, and leaves
objects the apply did not touch alone.

## Example

```
oath get snap:current
oath ls --kind snap
oath undo
```
