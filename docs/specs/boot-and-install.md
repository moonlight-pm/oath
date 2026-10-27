# Boot, images and install

Oath boots one kernel: vanilla kernel.org source, pinned by sha256,
compiled here with Oath's patches and config fragment. `oath-efi` is
the firmware menu. The image is a seed profile plus the packages it
names, packed on the build host. Installing wipes a named disk through
an installer ramdisk and writes machine facts from a host overlay.

**Status:** partly built. The kernel pin and version check, the menu,
boot rotation, profiles, overlays, the install flow, the `esp`
build-host guard, the test harness that gates installs, switch_root,
`panic=10` and the orderly shutdown are built, and so are immutable
build outputs that install and `esp` take by id.

Replaces: T27 (metal install), T38 (boot generations), the image parts
of T24, and D3 (seed as data).

## The one kernel (built)

- `pkg:linux` is a plan (`pkgs/linux/`): `linux-<kver>.tar.gz` from
  git.kernel.org pinned by sha256, `patches/*.patch`, and
  `linux.fragment` merged into `defconfig`, built by `pkg:gcc` in the
  build sandbox. The product is `boot/vmlinuz`,
  `boot/config` and `lib/modules/<release>/` with the depmod index. The
  pin and version change together.
- `cargo make` takes the kernel from `pkgs/lock.json` (or `OATH_KERNEL`
  and `OATH_MODULES`). There is no Nix kernel fallback.
- Pack reads the release string from the bzImage header and refuses
  the image unless `modules/<release>/` exists and a module's vermagic
  names the same release.
- Driver patches are first-class: a generation gap in a driver is fixed
  in `pkgs/linux/patches/`, gated on chip family so one kernel ships
  everywhere.

## Images and profiles (built)

- A **profile** is a JSON file in `crates/oath-core/seed/`: the objects
  of a fresh catalog and their first desired documents. `extends`
  layers profiles. `fixed` lists packages that can never be removed.
- `base` is a headless machine reached over SSH: host, net (DHCP), ssh,
  serial, screen, sshd, time, and 39 packages: glibc, the GNU userland
  (R18), util-linux, iproute2, iputils, dhcpcd, chrony, btrfs, oath,
  openssh (sshd, R36), dosfstools, curl, vim and the libraries they load. It carries no test fixtures: the image
  the harness checks is the image `install` ships.
- `desk` extends `base` with the graphical session (R20): `svc:seatd`,
  `svc:rivulet`, and rivulet's 35 packages (Mesa, wlroots, Xwayland,
  `pkg:gcc` for libstdc++); `svc:screen` is off. A check case may name
  the profile it is for (`profile = "desk"`).
- `cargo make build --profile <p>` builds `pkg:oath` (static, against
  glibc), packs the profile's present packages (each with `needs` for
  the libraries it loads, from the lock), seeds `stage/oath` with
  `oath seed --profile <p>`, hashes the store, and writes a btrfs `@`
  into a build output's `oath.qcow2` (`build/out/<id>/`, below), with
  the boot initrd and the installer initrd.
- A **host overlay** is the same JSON shape with only `objects`; each
  object is a JSON merge patch on desired. Machine facts live there
  (hostname, timezone, a backup target), never in a profile. Each
  patched document must pass the `oath set` checks, and the overlay
  counts as applied (`applied.json` for svc, host, net and ssh),
  because PID 1 boots from applied specs. A
  package an overlay turns on is linked by the first `oath apply`.

## Boot path (built)

- ESP: `EFI/BOOT/BOOTX64.EFI` is `oath-efi`; `vmlinuz` and `initrd.gz`
  are the current boot; `loader/loader.conf`, `loader/entries/*.conf`
  (BLS) and `loader/oath-boots` list the choices. `oath-efi` is the
  only loader on the ESP, USB stick and installer.
- `oath-efi` draws the mark with GOP, shows the menu (timeout 5 s on
  metal, 0 in QEMU), and starts the kernel with `LoadImage`.
- The kernel command line names the root (`oath.root=`) and subvolume
  (`oath.subvol=`): `@`, `@boot-N` or `@gen-N`. Anything else falls back
  to `@`. `install` names the root `UUID=<fsid>`, a UUID it gives the
  btrfs filesystem (`mkfs.btrfs -U`), since a disk's node is the order it
  was found in; a device node (`/dev/sda2`, canto's) still works. PID 1
  reads the btrfs superblocks itself (the initrd has no blkid) and waits
  up to 30 s for the root to appear, loading drivers again for what came
  meanwhile: a USB disk shows up seconds after its drivers load. Metal BLS entries and QEMU both carry `panic=10`,
  so a PID 1 panic reboots after ten seconds instead of hanging.
- The initrd's `/init` is PID 1. It mounts the root subvolume and
  switch_roots into it (moves `/dev`, `/proc`, `/sys`, `/run` across,
  moves the root over `/`, chroots): no `/newroot` is left behind. The
  initramfs modules and firmware stay bind-mounted for the deferred
  driver pass (services.md).
- Shutdown is PID 1's: `reboot`, `halt` and `poweroff` (links to PID
  1's own binary), Ctrl-Alt-Del and `oath` `power=` all signal it, and it stops
  services, syncs, unmounts the btrfs top level and remounts `/`
  read-only before calling the kernel.
- **Last five boots.** `cargo make esp --esp <part> --confirm`, run on
  the Oath machine, archives the current kernel and initrd under
  `/oath/boot/<id>/`, snapshots `@` as `@boot-<id>`, writes the new
  files and BLS entries, and keeps five archives. Ids are never reused.
  Catalog generations (`@gen-N`) are separate and belong to apply and
  undo.

## Install (built)

`cargo make install --disk <node> --confirm` with `--target user@host`,
`--qemu` (OVMF rehearsal on a scratch qcow2) or `--usb` (write an EFI
installer to a removable stick, which the tool checks is removable).
Optional `--hostname` and `--overlay <file.json>`. Every command on the
target is a script fed to its `/bin/sh` on stdin (under `sudo -n` for a
non-root user) and files go over with `dd of=`, so the login shell does
not matter (canto's September image logged `home` into Thoxa).

1. Refuse without `--confirm` and a `/dev/` node. Read and test the
   overlay before anything is wiped. Refuse a `--target` that is the
   build host itself: its `/proc/sys/kernel/random/boot_id` matches
   ours (built 2026-10-16). (`esp` already refuses to run on a machine
   that is not Oath.)
2. Bake the build host's SSH public keys into the installer initrd.
3. Enter the installer on the target: an EFI one-shot boot entry when
   the target has one, otherwise kexec (the native `pkg:kexec` binary,
   copied to the target). The installer is the same
   kernel with `oath.install`; PID 1 brings up a NIC with DHCP, starts
   OpenSSH's sshd for root (a host key made that boot, its unauthenticated
   process as `void`), and waits.
4. Over SSH, with the installer's native tools: `wipefs` and `sfdisk`
   write a GPT with a 512 MiB ESP and a btrfs partition; `mkfs.fat` and
   `mkfs.btrfs` format them; `@` gets the staged root through GNU
   `tar`; fix ownership and the setuid bit on `/lib/oath/sudo`.
5. Write the catalog overlay (hostname, the build host's keys for
   `home`, then the user's overlay) with
   `oath --root /mnt/oath seed --overlay`. Make the new system's host
   key with the installer's `ssh-keygen` and pin it in
   `build/known_hosts`.
6. Write the ESP and BLS, unmount, reboot, and wait for
   `home@<host>` to answer with an INDEX, accepting only the pinned
   host key.

## Known defects (from the 2026-10-13 review)

- Stage 0 of the toolchain is compiled with a floating nixpkgs
  (`pkgs/bootstrap.nix`). Everything that ships, the kernel and the
  Oath binaries included, is a pinned plan product (builds.md).

## Update (built)

`cargo make update --target home@host --confirm [--reboot] [--esp
<node>] [--output <id>]` moves an installed machine to a build output
without wiping it (built 2026-10-16), over SSH as `home` with `sudo`,
trusting only the host key `install` pinned:

1. Refuse unless the output passed `check` (or `--unchecked`), the
   target has `/oath/INDEX.md`, is not this machine (`boot_id`), is
   in-sync, and has nothing on probation.
2. Mount the ESP (default: partition 1 of the disk in `oath.root=`,
   through `findfs` for a `UUID=`),
   plan the rotation as `esp` does, and snapshot `@` as `@boot-<id>`
   first, so the archived boot pairs the old kernel with the old
   userspace.
3. Ship every store realization of the output that the machine lacks:
   a tar stream into `<slot>/.<hash>.part`, renamed into place.
4. `oath set pkg:<name>` to the output seed's `hash` and `needs` for
   each package the machine has whose pin or needs differ, then one
   `oath apply`: one generation, undoable, and apply re-hashes each
   tree before linking it. `needs` moves too: a package whose own tree
   is unchanged still pins what it runs with (on 2026-10-19 a rebuilt
   `pkg:sola` left `pkg:sola-browser` pinning the old one, and apply
   refused). A package the output fixes that the machine lacks is made
   (`oath new`) in the same apply, since the image's own files need it
   (`/etc/passwd` names `pkg:zsh`'s shell, R27); other objects the
   machine lacks are reported, not created.
5. Replace the image's files outside the catalog (`/lib/oath`,
   `/etc/passwd`, `group`, `shadow`, `shells`, `nsswitch.conf`,
   `ssh/ssh_config`, `/sbin/init`, `/bin/sudo`), each written beside
   itself and renamed over.
6. Archive the ESP's kernel and initrd as boot `<id>`, prune beyond
   five (and their `@boot-*`), write the new kernel, initrd, `oath-efi`
   and entries.
7. With `--reboot`: reboot through PID 1, wait for a new `boot_id`,
   require `/proc/1/exe` to be the output's PID 1 and `oath diff` to be
   in-sync.

`--qemu <older output>` rehearses it: `install --qemu` of the older
output, an OVMF boot, the update, a reboot and the checks. On
2026-10-16 it moved `30ae9c0094cd` to `7d6e132eb1cb`, and
`b0e9cee21d59` (from before R14) to `f4bc274b6ee4`, whose PID 1
migrated the old catalog at boot (20 objects) and came up in sync.

## Target

None open short of the desk (phase 8).

## Build outputs (built)

`cargo make build` packs into `build/out/.<id>.part/` and renames it to
`build/out/<id>/` when complete: `manifest.json` (profile, kernel
release, every pack's pin, licence and sources), the qcow, kernel,
initrds, `oath-efi`, the installer tree and the stage. The id is the
manifest's hash without the commit, so an unchanged tree reuses its
output. Nothing writes into an output afterwards except a clean
`check`, which adds `check-pass`. `install` and `esp` take
`--output <id>` (default: the newest checked output; `install --qemu`
and `--unchecked` take the latest) and read only that output; install
bakes the host's keys into a copy of the installer ramdisk in its run
directory. `run`, `up`, `start` and `check` boot `build/out/latest`.
Five outputs are kept, and the newest checked one.

## Test harness (built)

`cargo make check` boots an output's `oath.qcow2` (the latest, or
`--output <id>`) once per case in headless
QEMU (four at a time, 1 GiB each, a throwaway SSH key over fw_cfg),
serves the fetch canary at the guest's 10.0.2.2:18765, and runs the
steps in `check/cases.toml` over SSH as `home`. A case makes the
objects it needs with `oath new` (`svc:hold`, `pkg:fetchme`); the seed
has none. A `reboot` step reboots through the catalog, `reboot -f`, or
plain `reboot`, waits for QEMU to exit (it runs with `-no-reboot`) and
boots the same disk again.
A case marked `gap` is a known failure assigned to a phase; it may fail,
and the run fails if it starts passing so the mark gets removed. A clean
full run writes `check-pass` into that output (sha256 of kernel, initrd
and image); `cargo make install` refuses an output without a matching
stamp unless given `--unchecked`. `install --qemu` is itself a test and is not gated.

`cargo make check --target user@host` runs the cases marked `metal` on
an installed machine, one at a time, trusting only the host key in
`build/known_hosts`. The machine must start in-sync. After each case the
harness undoes back to the generation the case started from and
requires `oath diff` to be in-sync again, or it stops. A reboot step
waits for a new `boot_id`. canto (Mac Pro 2013), installed from the
image without fixtures, passed all 24 metal cases on 2026-10-15.

## Courage test

`cargo make install --qemu --disk /dev/vda --confirm` formats a scratch
disk, boots it through OVMF, and the installed system answers
`oath ls` over SSH as `home`. On metal (canto, 2026-10-14) the same
command run over SSH from the build host wipes the machine, reboots it
into the installer through the firmware, and the installed system passes
`cargo make check --target` unattended. The menu lists the current boot
and at least one archive after the first `cargo make esp`, and choosing
an archive boots that kernel with its `@boot-N`.
