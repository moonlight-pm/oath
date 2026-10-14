# Devices

Devices are hardware the system found, recorded as `dev:*` objects, and
the kernel modules and firmware that make them work. Today this is a
small fixed inventory and hard-coded module lists. The target is
discovery: modules load by modalias, devices appear as objects on
their own, and device-node and mount rules are fields.

**Status:** partly built. The `dev` kind and two-pass module loading
are built. Discovery and modalias loading are the target.

Replaces: T13 (devices), the module and firmware parts of T27 and T38.

## `dev` kind (built)

- Seeded objects: `dev:vda`, `dev:net0`, `dev:ttyS0`. Desired is
  `{ "present": true }`; `present: false` is refused.
- Actual is `present`, `class` (`block`, `net`, `tty`) and `node`
  (`/dev/vda`, `/sys/class/net/net0`, `/dev/ttyS0`). Converge probes
  `/dev` and sysfs for that name. It does not load modules.
- The names are QEMU's. On metal the seeded `vda` is absent. Unknown
  names are probed as `/dev/<name>`.
- `net:net0` is the link configuration; `dev:net0` is the inventory
  entry for the same NIC.

## Modules and firmware (built)

- The image packs the modules listed in `MODULE_ROOTS` (virtio,
  storage, common Ethernet, amdgpu, USB HID, HDA sound, Bluetooth, NFS)
  plus their dependencies from `modules.dep`, uncompressed, with a
  `load-order` file.
- PID 1 loads them in two passes with busybox `insmod`. The first pass
  skips display (KMS) and sound drivers so the NIC and SSH come up
  first; on some hardware loading amdgpu early stalls the network. The
  second pass runs after services start.
- amdgpu gets `si_support=1 cik_support=1` so Southern Islands and Sea
  Islands cards use it.
- Firmware is `pkg:firmware`: linux-firmware blobs from tag 20260916,
  each pinned by sha256 in `pkgs/firmware/pkg.toml` (today tg3 and SI
  Pitcairn for amdgpu and radeon). It is copied into the initrd and
  bind-mounted over the root's `/lib/firmware` so the deferred pass can
  load it. A machine that needs another blob gets a line there.
- `mdev -s` populates `/dev`. There is no udev. After every `mdev -s`
  PID 1 sets `/dev/null`, `zero`, `full`, `tty`, `ptmx`, `random` and
  `urandom` back to mode 0666, because `mdev -s` recreates them
  root-only.

## Unix floor (built)

Not catalog objects: tmpfs on `/tmp`, `/dev/shm`, `/run`; cgroup2 on
`/sys/fs/cgroup`; `/dev/pts` with mode 666 so `home` can open a PTY;
`/dev/fd` and the stdio links; a fixed `/etc/machine-id`.

## Known defects

- Two hard-coded module lists (one in pack, a fallback in PID 1) decide
  what hardware works. A machine with a NIC outside the list has no
  network.
- `dev` objects are seeded with QEMU names, so a metal catalog lies
  until someone edits it.

## Target

- **Modalias loading.** PID 1 walks sysfs for `modalias` files and
  loads the matching modules from `modules.alias`, for devices present
  at boot and for hotplug events. The pack ships every module the
  kernel config builds; the lists go away. The early/deferred split
  becomes a rule on module class (display and sound wait for the
  network), not a list.
- **Discovery.** Converge creates `dev:*` for what it finds, named by
  stable rules (block by bus path, NICs as `net0`, `net1`, …). The
  seed has no device objects.
- **Rules as fields.** Ownership and mode of device nodes (for a future
  graphical seat) and filesystems to mount (for a payload's data
  volume) become fields on catalog objects, not code in PID 1.

## Courage test

On a machine with hardware the image has never seen named, the NIC
comes up and `oath ls --kind dev` lists the disk and NIC with real
nodes.
