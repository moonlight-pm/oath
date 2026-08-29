# Generations

Undo is a filesystem snapshot, not a journal that is blind to the disk.

## Layout

The system disk is **btrfs**.

| Subvolume | Role |
|-----------|------|
| `@` | Live root, mounted at `/` |
| `@gen-N` | Read-only snapshot of `@` taken **before** that apply |

PID 1 mounts the btrfs top-level (subvolid=0) at **`/oath/run/fs`**:

```
/oath/run/fs/@          same tree as /
/oath/run/fs/@gen-1     generation 1
/oath/run/fs/@gen-2     …
```

Generations are **siblings** of `@`, not nested under `/`.

`snap:current` holds the last snapshot id. `snap:N` is a read-only
record (`generation`, `parent`, `time`, `reason`). Do not `set` `snap:N`.

## Apply and undo

`oath apply` snapshots `@` → `@gen-N`, then converges. `N` is never
reused (it is `max(existing)+1`, not `parent+1`).

`oath undo` restores the **catalog** (`objects`, `schema`, `log`,
`INDEX.md`, `store/`) from that generation, reapplies hostname, and
converges `pkg:*` links and `svc:*`. It does **not** replace `/oath/run`
(mounts and the init socket stay).

Picking an arbitrary old generation as the boot default is **confirm**
class. There is no bootloader menu yet. `undo` is the supported rewind.

qcow2 snapshots on the QEMU host are debug only. They are not this
mechanism.
