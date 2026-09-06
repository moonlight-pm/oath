# Generations

Undo is a filesystem snapshot, not a journal that is blind to the disk.

## Layout

The system disk is **btrfs**.

| Subvolume | Role |
|-----------|------|
| `@` | Live root, mounted at `/` |
| `@gen-N` | Read-only snapshot of `@` taken **before** that apply |
| `@boot-N` | Read-write snapshot of `@` taken when the ESP is rotated (firmware menu) |

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

`oath undo` is catalog rewind. Firmware rewind is the **boot menu**
(T38): `oath-efi` lists current plus the last five archived
kernel+initrd pairs. Timeout 5 s on metal (0 on QEMU EFI). Up/Down/Enter.
An archive boots `oath.subvol=@boot-N` (matching userspace snapshot).
Write a new slot with `cargo make esp --esp /dev/sda1 --confirm` (does
not format the disk). QEMU `run`/`probe` still boot `-kernel` and have
no menu.

Off-box copy is `svc:backup` (T33): one NFS file, overwritten, not a
second undo. Daily at 04:00 US Mountain. Snapshot is crash-consistent
(`sync` + CoW); packs may add `libexec/oath-backup-quiesce`.
`oath undo` does not restore from the NAS.

qcow2 snapshots on the QEMU host are debug only. They are not this
mechanism.
