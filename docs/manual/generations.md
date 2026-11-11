# Generations

A generation is a filesystem snapshot. Undo puts back from it the spec
of the objects one apply touched and converges them; it does not roll
the disk back.

## Layout

The system disk is **btrfs**.

| Subvolume | Role |
|-----------|------|
| `@` | Live root, mounted at `/` |
| `@gen-N` | Read-only snapshot of `@` taken **before** apply N converges |
| `@boot-N` | Snapshot of `@` taken when `oath update` or `cargo make esp` archives a boot |
| `@home` | Mounted at `/home` unless `host:local.mounts` names that path; in no generation or boot snapshot. Only `oath install` makes it: without it `/home` is part of `@` |
| `@work` | Mounted at `/work`, in no generation or boot snapshot. PID 1 makes it on the first boot that lacks it |

PID 1 mounts the btrfs top level (subvolid=0) at `/oath/run/fs`:

```
/oath/run/fs/@          same tree as /
/oath/run/fs/@gen-1     generation 1
/oath/run/fs/@gen-2     …
```

Generations are siblings of `@`, not nested under `/`.

A machine keeps the newest five generations, every one it applied or
undid back to since it booted, and the one on probation. PID 1 removes
the rest once a boot (`oath reap`, R62), and undo stops at the last one
kept (`no snapshot of generation N: it is past what undo can reach`).

`oath reap` says what holds each version that stays
(`pkg:chrony sha256-2b9386c5aeb0 stays: live, pin`) and what would go,
`oath reap --json` lists every version with what holds it (`held`), and
`sudo oath reap --confirm` removes what would go. A package or cell
version stays while something names it: its pin or live link, another
package's `needs`, the release the machine runs, a plan's needs, or a
kept generation. A tree no pin names yet waits a day for one
(`--arriving <secs>`). Nothing goes while a build runs. The space comes
back once no `@gen-N` or `@boot-N` holds the files.

A generation is all of `@`, so every apply pins whatever is there until
that `@gen-N` is gone. Keep fast-changing
bulk, such as a cargo `target/`, on another disk or in a nested
subvolume, which a snapshot leaves out.

`snap:current` is the generation the machine is at: the newest apply's,
or after an undo the parent of the apply it took back. Only apply and
undo move it: `oath set` refuses it (`snap:current is read-only: each
apply moves it to the generation it takes`). `oath undo` goes back one
apply at a time; the [boot menu](#boot-menu) boots an archived boot, not
a generation.
`snap:N` is a read-only record (`generation`, `parent`, `time`,
`reason`) in its desired document, and it stays after `@gen-N` goes;
`oath set` refuses it too.

A generation holds no status: status lives on the `/oath/status`
tmpfs, so no `@gen-N` or `@boot-N` carries it, and an archived boot
observes its own.

## Apply and undo

`oath apply` first checks everything it can without touching the
machine: nothing is on probation, every selected object parses as its
kind, confirm-class changes have `--confirm`, there is no `wants`
cycle, the package `needs` graph holds, the machine would have what
each present cell `needs`, and each selected cell that stays present has
its tree in the store, hashing to its pin. A cell comes from no origin,
so apply cannot fetch it again: a cell whose tree is gone or found
damaged drifts in `tree` and stops a plain `sudo oath apply` until
`sudo oath store cell:<name> <dir|file.tar>` puts it back. A refusal
takes no generation, changes nothing, and is logged with
`"result":"refused"`. Then it snapshots `@` to
`@gen-N` (holding what was applied before, not your pending edits) and
converges. `N` is never reused (it is `max(existing)+1`). The log line
says `ok`, or `error` if converging failed after the snapshot.

`oath undo` works like a stack. It takes the newest apply, `ok` or
`error`, that has not been undone yet, and puts back the spec of the
objects that apply touched from its `@gen-N`. An object that apply
created (`oath new`, or `oath add` and each need it brought) is removed;
one it removed (`oath rm`) comes back. Then it converges those objects
(hostname, `pkg:*` and `cell:*` links, `net:net0`, `ssh:local`,
`dev:*`, and `svc:*` through PID 1), sets
`snap:current` back, and logs `undo:N`. Run it again to step back one
more apply.

Undo does not touch the apply log, the package store, the SSH host
keys, or objects the apply did not touch. It keeps a package that the
image fixes even when that apply created it (`oath update` makes one a
machine lacks): the image's own files need it. A pending `oath set` on some
other object survives an undo.

An apply that can cut off SSH is on probation (`/oath/probation.json`)
until a new session runs `oath keep`. If its time runs out, or the
machine boots again first, PID 1 runs the same undo itself (logged
with tty `probation`).

A reboot or halt through the catalog (`power=reboot`) on its own takes
no generation: power is not configuration, so there is nothing for undo
to take back. It is logged `ok` without a generation, and PID 1 shuts
down cleanly. Applied together with other changes, it is part of their
generation. Plain `sudo reboot` reboots the same way.

## Boot menu

On metal, `EFI/BOOT/BOOTX64.EFI` is `oath-efi`. It sets the native GOP
mode, paints the Oath mark, and shows a 5 s menu: the current kernel
plus the last five archived kernel+initrd pairs. Up/Down/Enter; Esc
boots the current kernel. With no archive there is no menu. An
archive boots `oath.subvol=@boot-N`, the userspace snapshot taken when
it was archived. An archive whose `@boot-N` cannot be mounted boots `@`
instead, on the archive's kernel; `oath logs` says
`subvol @boot-N failed, falling back to @`. `oath-efi` is the only
loader; there is no fallback loader on the ESP.

## What the boot screen says

Under the mark, a line says what the machine is doing and a bar moves
while it works. A machine that stops booting shows where:

- `Reading the kernel` or `Reading the initrd`, with the bar still: the
  firmware cannot read the disk; try another port.
- `Starting Linux <version>` and nothing more: the kernel stopped before
  Oath's PID 1 ran.
- `Loading drivers: <name>` with the bar still: that driver hung.
- `Waiting for the root disk … Disks: …`: the disk Oath runs from has
  not appeared; the list is what the kernel sees.
- Amber text: the boot failed, and says why. `Boot failed: …` is PID 1's,
  and a shell waits on the serial console. `PID 1 failed: …` is its panic,
  and the kernel reboots 10 s later. Any other is the loader's
  (`Cannot read the kernel …`, `The firmware would not load the kernel …`):
  after 10 s it returns to the firmware, which tries its next boot option.
- `Oath is running, with no driver for this … display (vendor:device).
  Reach it with ssh home@<address>`: the boot finished, but Oath has no
  driver for the graphics card.

Once a display driver loads, the screen is the desk's or tty1's.

`cargo make esp --esp <vfat node> --confirm [--root <root>]
[--output <id>]` (`--root` is the `oath.root=` of the boot entries it
writes, a node or `UUID=…`; default: the running kernel's, the UUID an
install wrote) takes the kernel, initrd and `oath-efi` of a build
output (default: the newest one `cargo make check` passed;
`--unchecked` for another), archives the live ESP kernel as a new boot,
keeps five, snapshots `@` to `@boot-N`, and copies the new bits. The
new entries keep the options the current entry added, as `oath update`
does. It does not format anything. It
mounts the node you name **on the machine where you run it**, so it
refuses to run on a machine that is not Oath (no `/oath/INDEX.md`),
such as a NixOS build host.

QEMU `cargo make run` / `up` / `start` boot with `-kernel` and have no
menu. `cargo make install --qemu` boots through OVMF with a 0 s
timeout.

qcow2 snapshots on the QEMU host are debugging aids, not this
mechanism.
