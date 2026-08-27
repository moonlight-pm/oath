# QEMU appliance (limited)

**Status:** partial. Serial QEMU only. Not an installer. Not bare metal.

Build tools (kernel, busybox, qemu, musl cc) are borrowed for the host
build. They are not the runtime identity.

## Build and run

Needs: Rust with `x86_64-unknown-linux-musl`, loop-mount as root (`sudo`
for packing the disk), btrfs-progs, qemu.

```sh
nix-shell          # optional: kernel, qemu, musl cc, busybox, btrfs
./image/build.sh
./image/run.sh     # serial on stdio; KVM if /dev/kvm is usable
```

You land on a root shell. The catalog is `/oath`. Try `oath` with no
arguments, then `oath ls`.

## Probe (scripted)

```sh
./image/probe.py
```

Writes `build/runs/<id>/`: `meta.json`, `serial-boot*.log`, `events.jsonl`
(`oath-tel` lines), `probe.json`, `REPORT.md`. Uses a qcow overlay so
the golden image is not mutated.

Interactive `./image/run.sh` also creates a run dir and tees serial to
`serial.log`.

Guest telemetry is JSON on stderr, prefixed `oath-tel `, and appended
under `/oath/log/` once the disk is mounted.

## What works here

- Boot to serial, `oath` verbs
- Hostname apply and undo
- `power=reboot` without `--confirm` refuses (exit 3)
- Hostname **survives reboot** (probe boot2)

## Gaps

- No SSH, no installer, no packages.
- Generation subvolumes still nest under `/.oath-gens` on `@`.
