# Devices

Devices are hardware the system found, recorded as `dev:*` objects, and
the kernel modules and firmware that make them work. Devices appear as
objects on their own, and modules load by modalias at boot and on
hotplug. Filesystems to mount are `host:local.mounts`; device-node rules
as fields are the target.

**Status:** built, except device-node rules as fields. Discovery and
hotplug were built 2026-10-16, mounts 2026-10-23.

Replaces: T13 (devices), the module and firmware parts of T27 and T38.

## `dev` kind (built)

- The seed has no device objects. PID 1 discovers them from sysfs at
  boot (under the boot lock, after `net0` is named) and again a second
  after a `block`, `net` or `tty` uevent settles (taking the lock
  without waiting; if an apply holds it, two seconds later). Each found
  device gets an object (desired `{ "present": true }`; `present:
  false` is refused); an object whose device is gone is deleted.
- Names are stable: a disk is `disk-` plus its bus path with `:` as
  `.` (`disk-pci-0000.00.1f.2-ata-1`, `disk-pci-0000.00.04.0` for
  virtio, `…-nvme-1`, `…-usb-1-2`); the NIC PID 1 renamed is `net0` and
  any other physical NIC `net1`, `net2`, … in kernel-name order; a
  serial port keeps its kernel name (`ttyS0` when a UART answers,
  `ttyUSB0`, `ttyACM0`). Loop, RAM, zram, dm, md and nbd devices are
  not listed.
- Actual: `present`, `class` (`block`, `net`, `tty`), `node`
  (`/dev/sda`, `/sys/class/net/net0`), the bus `path`, the bound
  `driver`, and `size` (bytes), `model`, `removable` for a disk, `mac`
  for a NIC.
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
- devtmpfs populates `/dev`. There is no udev. After every discovery
  pass PID 1 sets `/dev/null`, `zero`, `full`, `tty`, `ptmx`, `random`
  and `urandom` to mode 0666, whatever devtmpfs made them, and the GPU
  render nodes (`/dev/dri/renderD*`) too, again whenever a `drm` device
  is added: GL clients open them themselves. Sound nodes (`/dev/snd/*`)
  are root:home 0660, again whenever a `sound` device is added, so the
  desk's PipeWire opens them as `home` (R22). Game controllers are too
  (R30), again whenever an `input` or `hidraw` device is added: each
  input device joydev took (it has a `js*`), its `js*` and `event*`
  nodes, and the `hidraw*` of its HID device; `/dev/uinput` is a static
  node (c 10:223), root:home 0660, whose first open loads `uinput`
  through the kernel's `char-major-10-223` request. `/dev/kvm` is
  root:home 0660 too (R33), so a build host's QEMU runs as `home` with
  KVM; devtmpfs makes it when coldplug loads `kvm-intel` or `kvm-amd`
  for the CPU, and a machine without virtualization has none. Display (`card*`)
  and other input nodes stay root's 0600; seatd opens those for the
  desk (R20).

## Hotplug (built)

PID 1 opens a `NETLINK_KOBJECT_UEVENT` socket before the first
coldplug (4 MiB buffer, so boot's burst is kept) and drains it every
loop. An `add` with a `MODALIAS` loads what the alias resolves to, with
the coldplug index, display and sound included (boot is over); each
load is logged `hotplug: <module> for <alias>`. Block, net and tty
events schedule discovery. devtmpfs makes the nodes. Every kernel
uevent is passed on, unchanged, to netlink group 4, where udevd would
rebroadcast it: libudev-zero's monitors listen there, so libinput sees
input devices come and go and wlroots sees a GPU appear.

## Mounts (built)

`host:local.mounts` maps a path under `/mnt` or `/home` to a `source`
(`LABEL=`, `UUID=`, `PARTLABEL=` or `PARTUUID=`; never `/dev/sdX`,
which moves between ports and boots) and mount(8) `options`. util-linux
does the work: `findfs` says whether the source is plugged in, `mount`
and `umount` act. A mount is outside `@`, so no generation holds what
is on it.

- At boot, after `dev:*` discovery, PID 1 mounts each entry whose
  source is present (making the directory) and logs `mount: <path>
  (<source>)`; one whose disk is absent is logged `waits for`. After
  every later discovery (a block uevent, then a second's settle) it
  mounts what appeared. It unmounts nothing until shutdown, which
  unmounts them deepest first before `/` goes read-only.
- `oath apply` mounts new entries, remounts one whose options changed
  (an option taken out stays until the next mount), and unmounts one
  that left the map or changed its source; a busy path is an apply
  error. An absent disk is not: apply prints `<path> waits for
  <source>`. `oath undo` converges the same way.
- Status (`oath get host:local --actual`) lists only what is mounted.

## Unix floor (built)

Not catalog objects: tmpfs on `/tmp`, `/dev/shm`, `/run`; cgroup2 on
`/sys/fs/cgroup`; `/dev/pts` with mode 666 so `home` can open a PTY;
`/dev/tty1` owned by `home` (0600) for `svc:screen`;
`/dev/fd` and the stdio links; a fixed `/etc/machine-id`.

## Known defects

- NICs other than `net0` are listed but not configured: one NIC, until
  a machine needs more (network-and-ssh.md).
- A disk pulled out while mounted stays in the mount table until
  shutdown, or an apply that takes its entry out.

## Target

- **Rules as fields.** Ownership and mode of device nodes (sound for
  the desk) become fields on catalog objects, not code in PID 1.

## Courage test

On a machine with hardware the image has never seen named, the NIC
comes up and `oath ls --kind dev` lists the disk and NIC with real
nodes.
