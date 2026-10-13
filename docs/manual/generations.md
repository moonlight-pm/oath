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

`oath apply` first checks everything it can without touching the
machine: every selected object parses as its kind, confirm-class
changes have `--confirm`, there is no `wants` cycle, and the package
`needs` graph holds. A refusal takes no generation, changes nothing,
and is logged with `"result":"refused"`. Then it snapshots `@` to
`@gen-N` (holding what was applied before, not your pending edits) and
converges. `N` is never reused (it is `max(existing)+1`). The log line
says `ok`, or `error` if converging failed after the snapshot.

`oath undo` works like a stack. It takes the newest apply, `ok` or
`error`, that has not been undone yet, and puts back the spec of the
objects that apply touched from its `@gen-N`. An object that apply
created (`oath new`) is removed; one it removed (`oath rm`) comes back.
Then it converges those objects (hostname, `pkg:*` links, `net:net0`,
`ssh:local`, `dev:*`, and `svc:*` through PID 1), sets `snap:current`
back, and logs `undo:N`. Run it again to step back one more apply.

Undo does not touch the apply log, the package store, the SSH host
keys, or objects the apply did not touch. A pending `oath set` on some
other object survives an undo.

A reboot through the catalog (`power=reboot`) is itself an apply and
takes a generation.

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
machine where you run it**, so it refuses to run on a machine that is
not Oath (no `/oath/INDEX.md`), such as the build host.

QEMU `cargo make run` / `up` / `start` boot with `-kernel` and have no
menu. `cargo make install --qemu` boots through OVMF with a 0 s
timeout.

qcow2 snapshots on the QEMU host are debugging aids, not this
mechanism.
