# Generations

Undo is a filesystem snapshot, not a journal.

## Layout

The system disk is **btrfs**.

| Subvolume | Role |
|-----------|------|
| `@` | Live root, mounted at `/` |
| `@gen-N` | Read-only snapshot of `@` taken **before** apply N converges |
| `@boot-N` | Snapshot of `@` taken when `cargo make esp` archives a boot |

PID 1 mounts the btrfs top level (subvolid=0) at `/oath/run/fs`:

```
/oath/run/fs/@          same tree as /
/oath/run/fs/@gen-1     generation 1
/oath/run/fs/@gen-2     …
```

Generations are siblings of `@`, not nested under `/`.

`snap:current` holds the last snapshot id. `snap:N` is a read-only
record (`generation`, `parent`, `time`, `reason`). Do not `set` `snap:N`.

## Apply and undo

`oath apply` checks `wants` cycles and the package `needs` graph,
snapshots `@` to `@gen-N`, then converges. `N` is never reused (it is
`max(existing)+1`).

`oath undo` copies the catalog back from the last successful apply's
generation — `objects`, `schema`, `log`, `INDEX.md`, `store`, and
`ssh` — then converges `host:local`, `pkg:*` links, `net:net0`,
`ssh:local` and `dev:*` from the restored desired, and notifies PID 1
for `svc:*`. It does not replace `/oath/run` (mounts and the init socket
stay).

Known gaps (scheduled):

- A failed apply is not logged. `oath undo` after a failure reverts
  the previous **good** apply instead.
- Per-field checks run after the snapshot, so a refused apply can
  still leave a generation behind.
- Undo restores `log` and `store` along with the objects.

## Boot menu

On metal, `EFI/BOOT/BOOTX64.EFI` is `oath-efi`. It sets the native GOP
mode, paints the Oath mark, and shows a 5 s menu: the current kernel
plus the last five archived kernel+initrd pairs. Up/Down/Enter. An
archive boots `oath.subvol=@boot-N`, the userspace snapshot taken when
it was archived. systemd-boot stays at `EFI/systemd/` as a fallback.

`cargo make esp --esp <vfat node> --confirm [--root /dev/sda2]` packs
the kernel and initrd, archives the live ESP kernel as a new boot,
keeps five, snapshots `@` to `@boot-<id>`, and copies the new bits. It
does not format anything. It mounts the node you name **on the
machine where you run it**. Nothing stops that being the build host's
own ESP: never run it on the build host with that host's ESP.

QEMU `cargo make run` / `up` / `start` boot with `-kernel` and have no
menu. `cargo make install --qemu` boots through OVMF with a 0 s
timeout.

qcow2 snapshots on the QEMU host are debugging aids, not this
mechanism.
