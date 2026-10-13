# Boot, images and install

Oath boots one kernel: vanilla kernel.org source, pinned by sha256,
compiled here with Oath's patches and config fragment. `oath-efi` is
the firmware menu. The image is a seed profile plus the packages it
names, packed on the build host. Installing wipes a named disk through
an installer ramdisk and writes machine facts from a host overlay.

**Status:** partly built. The kernel pin and version check, the menu,
boot rotation, profiles, overlays, the install flow, the `esp`
build-host guard and the test harness that gates installs are built. An
immutable build output with a manifest is the target.

Replaces: T27 (metal install), T38 (boot generations), the image parts
of T24, and D3 (seed as data).

## The one kernel (built)

- `image/build-linux.sh` fetches `linux-<kver>.tar.gz` from
  git.kernel.org, checks it against the sha256 pinned in the script,
  applies `image/linux-patches/*.patch`, merges `image/linux.fragment`
  into `defconfig`, and installs `vmlinuz`, `modules/<release>/` and
  firmware into `build/linux/`. The pin and version change together.
- `cargo make` reads only `build/linux/` (or `OATH_KERNEL` and
  `OATH_MODULES`). There is no Nix kernel fallback.
- Pack reads the release string from the bzImage header and refuses
  the image unless `modules/<release>/` exists and a module's vermagic
  names the same release.
- Driver patches are first-class: a generation gap in a driver is fixed
  in `image/linux-patches/`, gated on chip family so one kernel ships
  everywhere.

## Images and profiles (built)

- A **profile** is a JSON file in `crates/oath-core/seed/`: the objects
  of a fresh catalog and their first desired documents. `extends`
  layers profiles. `fixed` lists packages that can never be removed.
- `base` is a headless machine reached over SSH: host, net (DHCP), ssh,
  serial, sshd, busybox, btrfs, oath, dropbear, curl, and the
  `pkg:hello` and `pkg:fetchme` canaries.
- `cargo make build --profile <p>` builds the musl binaries, packs the
  profile's present packages, seeds `stage/oath` with
  `oath seed --profile <p>`, hashes the store, and writes a btrfs `@`
  into `build/oath.qcow2`. It also writes the boot initrd and the
  installer initrd.
- A **host overlay** is the same JSON shape with only `objects`; each
  object is a JSON merge patch on desired. Machine facts live there
  (hostname, timezone, a backup target), never in a profile. Each
  patched document must pass the `oath set` checks, and the overlay
  counts as applied (svc `applied`, host and net `actual`, ssh
  `actual.authorized`), because PID 1 boots from applied state. A
  package an overlay turns on is linked by the first `oath apply`.

## Boot path (built)

- ESP: `EFI/BOOT/BOOTX64.EFI` is `oath-efi`; `vmlinuz` and `initrd.gz`
  are the current boot; `loader/loader.conf`, `loader/entries/*.conf`
  (BLS) and `loader/oath-boots` list the choices; systemd-boot is kept
  only as a fallback binary.
- `oath-efi` draws the mark with GOP, shows the menu (timeout 5 s on
  metal, 0 in QEMU), and starts the kernel with `LoadImage`.
- The kernel command line names the root device (`oath.root=`) and
  subvolume (`oath.subvol=`): `@`, `@boot-N` or `@gen-N`. Anything else
  falls back to `@`.
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
Optional `--hostname` and `--overlay <file.json>`.

1. Refuse without `--confirm` and a `/dev/` node. Read and test the
   overlay before anything is wiped.
2. Bake the build host's SSH public keys into the installer initrd.
3. Enter the installer on the target: an EFI one-shot boot entry when
   the target has one, otherwise kexec. The installer is the same
   kernel with `oath.install`; PID 1 brings up a NIC with DHCP, starts
   dropbear for root, and waits.
4. Over SSH: GPT with a 512 MiB ESP and a btrfs partition holding `@`;
   copy the staged root with tar; fix ownership and the setuid bit on
   `/lib/oath/sudo`.
5. Write the catalog overlay (hostname, the build host's keys for
   `home`, then the user's overlay) with
   `oath --root /mnt/oath seed --overlay`.
6. Write the ESP and BLS, unmount, reboot, and wait for
   `home@<host>` to answer with an INDEX.

## Known defects (from the 2026-10-13 review)

- Install copies whatever `build/stage` holds, which may be stale or
  from another profile.
- `cargo make esp` mounts a local ESP and snapshots the local `@`. On
  the build host it would write the build host's own ESP; only a note
  in the docs prevents that.
- The build is not reproducible from a clean checkout: tools come from
  a floating nixpkgs and the store cache is reused.
- Metal kernels boot without `panic=`, so a PID 1 failure hangs.

## Target

- **Immutable outputs.** Each build writes `build/out/<id>/` with a
  manifest (profile, kernel release, every package pin). Install and
  `esp` take an output id, verify the manifest, and never read a
  mutable stage.
- **Build-host guard (install).** `install` refuses a target that is
  the build host. (`esp` already refuses to run on a machine that is
  not Oath.)
- **Crash reboot.** Metal entries carry `panic=10`.
- **Native tools.** The installer's tools (sgdisk, mkfs.fat, btrfs,
  dropbear, tar) become native packages (packages.md).

## Test harness (built)

`cargo make check` boots `build/oath.qcow2` once per case in headless
QEMU (four at a time, 1 GiB each, a throwaway SSH key over fw_cfg),
serves the `pkg:fetchme` canary at the guest's 10.0.2.2:18765, and runs
the steps in `check/cases.toml` over SSH as `home`. A `reboot` step
reboots through the catalog, `reboot -f`, or plain `reboot`, waits for
QEMU to exit (it runs with `-no-reboot`) and boots the same disk again.
A case marked `gap` is a known failure assigned to a phase; it may fail,
and the run fails if it starts passing so the mark gets removed. A clean
full run writes `build/check-pass` (sha256 of kernel, initrd and image);
`cargo make install` refuses an image without a matching stamp unless
given `--unchecked`. `install --qemu` is itself a test and is not gated.

## Courage test

`cargo make install --qemu --disk /dev/vda --confirm` formats a scratch
disk, boots it through OVMF, and the installed system answers
`oath ls` over SSH as `home`. On metal, the menu lists the current boot
and at least one archive after the first `cargo make esp`, and choosing
an archive boots that kernel with its `@boot-N`.
