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
cargo run -p oath-make -- build
cargo run -p oath-make -- run
```

You land on a root shell. Then:

```
oath
oath ls
```

See [using.md](using.md), [catalog.md](catalog.md), [services.md](services.md),
[generations.md](generations.md).

## Probe (scripted)

```sh
cargo run -p oath-make -- probe
```

Writes `build/runs/<id>/`: `meta.json`, `serial-boot*.log`, `events.jsonl`
(`oath-tel` lines), `probe.json`, `REPORT.md`. Uses a qcow overlay so
the golden image is not mutated.

`cargo run -p oath-make -- run` also writes a run dir (`serial.log`).

Host orchestration is the `oath-make` crate. QEMU, mkfs.btrfs, and a
loop-mount (sudo) are still external tools.

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
