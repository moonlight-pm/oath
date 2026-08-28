# The appliance

**Today’s product:** an x86_64 QEMU machine with a serial console. Not
an installer. Not bare metal. Not a desktop.

Build tools (kernel, busybox, qemu, musl cc) are borrowed on the **host**.
They are not the runtime identity.

## Build and run

Needs: Rust with `x86_64-unknown-linux-musl`, loop-mount as root (`sudo`
for packing the disk), btrfs-progs, qemu.

```sh
nix-shell          # optional: kernel, qemu, musl cc, busybox, btrfs
cargo make build
cargo make run
```

You land on a root shell. Then:

```
oath
oath ls
```

See [using.md](using.md), [catalog.md](catalog.md), [services.md](services.md),
[generations.md](generations.md).

## Leave the VM

Ctrl-D (or `exit`) leaves the **shell**. `svc:serial` is `restart=always`,
so PID 1 starts it again. That is not leaving QEMU.

There is no QEMU monitor (`-monitor none`), so **Ctrl-A x** does not work.
Ctrl-C goes to the guest (`signal=off`).

From the guest, halt:

```
oath set host:local power=halt
oath apply --confirm
```

`cargo make run` also exits if the guest reboots (`-no-reboot`).

From the host, another terminal: stop `qemu-system-x86_64` (or the
`cargo make run` process).

## Probe (scripted)

```sh
cargo make probe
```

Writes `build/runs/<id>/`: `meta.json`, `serial-boot*.log`, `events.jsonl`
(`oath-tel` lines), `probe.json`, `REPORT.md`. Uses a qcow overlay so
the golden image is not mutated.

`cargo make run` also writes a run dir (`serial.log`).

Host orchestration is `cargo make` (crate `oath-make`). QEMU, mkfs.btrfs,
and a loop-mount (sudo) are still external tools.

## What is in the image

- Linux kernel (borrowed) + initramfs (`oath-init` as `/init`)
- btrfs disk, live subvolume `@`
- `/usr/lib/oath/init` — PID 1 after switch-root
- `/bin/oath`, busybox, `btrfs`
- Catalog at `/oath`

## Telemetry

Guest: JSON on stderr, prefixed `oath-tel `, and `/oath/log/*.jsonl`
once the disk is mounted.

## What works

- Boot to serial, `oath` verbs
- Hostname apply, undo, reboot (hostname survives)
- `power=reboot` without `--confirm` refuses (exit 3)
- Sibling generations `/oath/run/fs/@gen-N`
- `svc:hold` start / stop / undo / persist across reboot

## Limits

- No SSH, no installer, no packages, no network objects
- No boot-generation picker (undo is the supported rewind)
