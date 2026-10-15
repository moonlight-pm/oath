# Devices

Devices are hardware the system found, recorded as `dev:*` objects, and
the kernel modules and firmware that make them work. Today this is a
small fixed inventory, and modules load by modalias. The target is
discovery: devices appear as objects on their own, and device-node and
mount rules are fields.

**Status:** partly built. The `dev` kind and modalias loading are
built. Hotplug and discovery are the target.

Replaces: T13 (devices), the module and firmware parts of T27 and T38.

## `dev` kind (built)

- Seeded objects: `dev:net0`, `dev:ttyS0`. Desired is
  `{ "present": true }`; `present: false` is refused.
- Actual is `present`, `class` (`block`, `net`, `tty`) and `node`
  (`/sys/class/net/net0`, `/dev/ttyS0`). Converge probes `/dev` and
  sysfs for that name. It does not load modules. Unknown names are
  probed as `/dev/<name>`.
- The seed had `dev:vda`, QEMU's disk, until 2026-10-14: on metal it
  was absent, so a fresh install was never in-sync. Disks are not
  listed until discovery.
- `net:net0` is the link configuration; `dev:net0` is the inventory
  entry for the same NIC.

## Modules and firmware (built)

- The image ships every module the kernel config builds
  (`modules.order`, 179 today), uncompressed, with depmod's index:
  `modules.dep`, `modules.alias`, `modules.softdep`, `modules.builtin`.
  No list in the tree names a driver.
- **Coldplug.** PID 1 walks `/sys/devices` for `modalias` files,
  matches them against `modules.alias` (fnmatch), and loads each match
  with `finit_module` after its `modules.dep` dependencies and softdep
  `pre:` modules. It repeats while a pass loads something new (a bus
  driver makes new devices appear), up to eight passes. Display drivers
  that replace the firmware framebuffer (amdgpu, i915, xe, nouveau,
  radeon) and everything under `sound/` wait for a second coldplug
  after services start, so sshd is up first; on canto an early amdgpu
  stalls the NIC. Loaded and failed modules go to `oath logs`.
- **On request.** `/proc/sys/kernel/modprobe` is `/lib/oath/modprobe`,
  a link to PID 1's binary in the initrd and the root. The kernel runs
  it for a filesystem (`fs-btrfs` at the root mount, `fs-vfat`), a code
  page (`nls_cp437`) and the like. `backup-send` uses it for NFS.
- Parameters: amdgpu always gets `si_support=1 cik_support=1`;
  `module.key=value` on the kernel command line is passed to that
  module when it loads, as kmod does.
- Firmware is `pkg:firmware`: linux-firmware blobs from tag 20260916,
  each pinned by sha256 in `pkgs/firmware/pkg.toml` (today tg3 and SI
  Pitcairn for amdgpu and radeon). Modules and firmware are in the
  initrd and bind-mounted over the root's `/lib/modules` and
  `/lib/firmware` so the deferred pass can load them. A machine that
  needs another blob gets a line there.
- `mdev -s` populates `/dev`. There is no udev. After every `mdev -s`
  PID 1 sets `/dev/null`, `zero`, `full`, `tty`, `ptmx`, `random` and
  `urandom` back to mode 0666, because `mdev -s` recreates them
  root-only.

## Unix floor (built)

Not catalog objects: tmpfs on `/tmp`, `/dev/shm`, `/run`; cgroup2 on
`/sys/fs/cgroup`; `/dev/pts` with mode 666 so `home` can open a PTY;
`/dev/fd` and the stdio links; a fixed `/etc/machine-id`.

## Known defects

- No hotplug: a device that appears after the second coldplug (a USB
  stick plugged in later) gets no driver until the next boot.
- The inventory is two fixed names; the disk is not in it.

## Target

- **Hotplug.** PID 1 listens for kernel uevents and loads the driver a
  new device asks for, with the same index.
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
