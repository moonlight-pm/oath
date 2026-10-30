# Install

Wipe a named disk and copy the packed Oath tree onto it.

```sh
cargo make build
cargo make install --target user@host --disk /dev/sda --confirm \
  --hostname canto --overlay hosts/canto.json
```

`--confirm` is required. `--disk` is the **whole disk** (GPT). The live
OS on that host must be reachable over SSH as root, or as a user with
`sudo -n`. Every command is fed to the target's `/bin/sh` on stdin and
files are written with `dd`, so the user's login shell does not matter.

## What it does

1. Picks a build output: `--output <id>`, else the newest one that
   passed `cargo make check` (its `check-pass` must match the kernel,
   initrd and image). `--unchecked` takes the latest output without
   that; `--qemu` rehearsals take the latest and are not gated.
   Everything below comes from that one output; the build host's keys
   are baked into a copy of its installer ramdisk under
   `build/runs/<time>-install/`, never into the output.
   Reads and checks `--overlay` against a scratch catalog, before
   anything is wiped, and refuses a `--target` that is the machine it
   runs on (the same `boot_id`).
2. Boots an **installer ramdisk** on the target (`oath.install=1`,
   OpenSSH's sshd, the build host's keys for root). On EFI machines this is
   a one-shot loader entry; kexec is the fallback (Oath's own static
   `kexec`, copied to the target, so the live OS needs none).
3. Partitions GPT with the ramdisk's native tools (`wipefs`, `sfdisk`):
   a 512 MiB ESP (`mkfs.fat`), then btrfs (`mkfs.btrfs`) with subvolume
   `@` and a new UUID, which the boot entry names the root by
   (`oath.root=UUID=…`): the disk's node changes with the order disks
   are found in.
4. Copies the packed root tree (GNU `tar`).
5. Merges the catalog overlay with the ramdisk's `oath`
   (`oath --root /mnt/oath seed --overlay`): `host:local.hostname`
   (default `oath`), the build host's public keys as `ssh:local`, then
   your `--overlay` on top. Makes the new system's SSH host key
   (`/oath/ssh/host_ed25519`) and pins it in `build/known_hosts`.
6. Writes the ESP: `oath-efi` as `EFI/BOOT/BOOTX64.EFI` (the only
   loader), `vmlinuz`, `initrd.gz`, `loader/entries/oath.conf`, and
   `loader/oath-boots`. The kernel and the initrd modules are the same
   checked build.
7. Reboots and waits for SSH as `home`, accepting only the pinned host
   key, then runs `oath ls`.

`net:net0` stays `dhcp` from the seed.

## The overlay

Machine-specific facts live in an overlay, not in the seed. Each
object is a JSON merge patch on the seeded desired; a new id creates
the object. See [Catalog](catalog.md).

```json
{
  "objects": {
    "host:local": { "timezone": "MST7MDT,M3.2.0,M11.1.0" },
    "svc:serial": { "enabled": false },
    "svc:backup": {
      "exec": ["/lib/oath/backup-daily", "nas:/export/backup/canto"],
      "wants": [], "restart": "always", "enabled": true
    }
  }
}
```

An overlay that sets `ssh:local.authorized` replaces the build host's
keys rather than adding to them. Each patched object must pass the same
checks as `oath set`, and the overlay counts as applied: the machine
boots with it, no `oath apply` needed (except to link a package an
overlay turns on).

## Check the installed machine

```sh
cargo make check --target home@10.0.0.108
```

Runs the harness cases marked `metal` on the machine, one at a time,
over SSH with the key pinned at install. See [Build](build.md).

## Rehearse in QEMU

```sh
cargo make install --qemu --disk /dev/vda --confirm
```

That runs the same steps against a fresh qcow2 in QEMU with OVMF, then
boots it through `oath-efi`.

## Onto a USB disk plugged into this host

```sh
cargo make install --local --disk /dev/sdd --confirm \
  --hostname novus --overlay hosts/novus.json
```

For a machine that boots Oath from a USB disk, the build host itself
included. The disk must be removable, USB and not mounted. The installer
ramdisk runs in QEMU with the raw disk as its own (`/dev/vda`; the disk
is lent to your user for the run), does steps 3 to 6, then OVMF boots
the disk once, `oath ls` must answer, and it is powered off cleanly. Its
loader waits 5 s, as on metal. Boot it from the firmware's boot menu;
nothing changes the firmware's boot order. Such a stick is also an
installer: `oath install` (below).

## From a stick, on the machine: oath install

Boot an Oath stick (the one above) on the machine, open a terminal:

```sh
oath install                          # the disks: node, size, port, model, serial
sudo oath install /dev/sdb            # what it would erase and write; writes nothing
sudo oath install /dev/sdb --confirm  # wipe it and install
reboot                                # remove the stick first
```

The disk is a node (`/dev/sdb`, `sdb`) or a serial; `sudo oath install`
also shows each filesystem's type and label. Node names follow the
order the kernel found the disks in and can differ from another
system's, so check the model and serial the preview repeats.

`--confirm` wipes that disk: a GPT with a 512 MiB ESP and btrfs for the
rest, with `@` (the system) and `@home` (mounted at `/home`, outside
every generation and boot snapshot). It copies the system the stick
runs: its packages (NVIDIA's driver too, if the stick built it), its
settings (hostname, overlay, SSH keys it trusts) and its release, at
generation 0, with an empty home, a new SSH host key (printed at the
end), and none of the stick's history, password or pairings. It writes
the ESP, adds `Oath` to the firmware's boot list first and removes the
other systems' entries; the firmware's own (USB, network, setup) stay,
and nothing else in the firmware's variables is touched.

It checks no backups and reads no other disk: knowing what is on them
is yours. It refuses only the disk the stick runs from and a disk with
something mounted. Agents run the same over SSH (`--json` for each
step).

## USB installer stick

```sh
cargo make install --usb --disk /dev/sdd --confirm
```

The disk must be a removable USB device. Boot the stick's EFI entry
(hold Option on a Mac). The ramdisk gets a shell on `tty0` and DHCP on
the NIC with carrier. Then run the `--target` command from the build
host; it sees `/run/oath-install/ready` and skips step 2.

## Update an installed machine

Each machine updates itself from its origins (R38). The build host
publishes every output it checked (`cargo make publish`,
[Packages](packages.md)); on the machine:

```sh
oath update                           # the newest output for this machine, and what moves
sudo oath update --confirm --reboot   # move to it, then boot it
```

It takes the newest release for `host:local.profile` from the first of
`host:local.origins` that serves one a key in `host:local.publishers`
signed (the `base` profile trusts buccina's), and refuses one older
than the release the machine runs (`/oath/output.json`) unless
`--output <id>` names it. It also refuses pending changes and an apply
on probation, and a set of moves that would leave a package needing
another at a hash it will not have (the check `oath apply` makes, made
first, so the preview says it too). With `--confirm`, it:

1. fetches every pack it will pin that the store lacks, and builds each
   recipe the owner built (Steam, NVIDIA's driver) from the output's
   plan, before it changes anything: a fetch or build that fails (a
   network error is tried twice more), or a build that is not the
   output's pin, leaves the machine as it was, and running it again
   fetches the rest;
2. snapshots `@` as `@boot-<n>`;
3. writes the output's accounts (`/etc/passwd`, `group`, `shadow`) and
   `/var/empty`, since services that restart next drop to `void` (R36);
4. sets each package the machine has to the output's pin and needs,
   makes each package the output fixes that the machine lacks (the
   image's files need it: `/etc/passwd` names `pkg:zsh`'s shell) and
   each one a package it has present needs, down their needs (R41's
   `pkg:libglvnd`, which wlroots needs once Mesa leaves libEGL to it), and
   runs `oath apply` once (a name in `/bin` or `/lib` that moves from one
   package to another, `libEGL.so.1` there, moves in it whichever
   converges first)
   (`oath undo` takes it back but keeps a fixed package; other objects
   the output's seed has and the machine does not, a new base service
   say, are listed, not created: add them with `oath new`). A package
   the profile pins from elsewhere (R28) moves too while the machine's
   pin is the one the release it runs set (each release lists those
   pins, `pinned`); one its owner moved keeps the machine's own pin, and
   only its needs move to the output's hashes (a need on another such
   package the machine keeps stays at the machine's pin). A machine
   whose release lists none (`36de66963ac1` and older) keeps them
   all, as does a package the output does not carry (`pkg:dropbear`
   after R36, one its owner added). A recipe the owner built moves to
   the pin step 1 built. Each running service whose program moved
   restarts on it, except one with a `seat` (the desk's and the
   console's, R42), which keeps its program until it next starts: the
   desk takes its new programs at a reboot, or at Exit Sola and `oath
   desk`. With `--reboot` no service restarts, and the boot starts each
   on its new program. When sshd's program moved, the apply is on
   probation: the services restart then (the seat's still keep theirs),
   and the update waits until you open a **new** SSH session and run
   `sudo oath keep` there;
5. replaces `/lib/oath`, the image's static `/etc` files, `/sbin/init`
   and `/bin/sudo`;
6. archives the ESP's kernel and initrd as boot `<n>` (with the
   snapshot from step 2) and writes the new kernel, initrd and
   `oath-efi`, keeping five archives (`--esp`, default partition 1 of
   the root disk);
7. with `--reboot`, reboots. `oath update` afterwards says whether
   PID 1 is the output's.

A machine that still runs dropbear as `svc:sshd` (from before R36)
moves to OpenSSH after it has booted the new PID 1, which writes
`/etc/ssh/sshd_config` and carries dropbear's host key over: run `sudo
oath update --confirm` again, then `sudo oath keep` from a new SSH
session. If none gets in, PID 1 undoes it and dropbear serves again.

An `oath` from before step 1 (`pkg:oath` `sha256-6e563f2d6c69…` and
older) fetches during the apply instead; if a fetch fails there, the
machine is half moved and `oath update` refuses the pending changes.
`sudo oath apply` finishes them; the update run after it snapshots the
half-moved `@` as `@boot-<n>`, so move the first one aside and back if
the archived boot should keep the old userspace. Two builds of step 1
(`pkg:oath` `sha256-e3790fbd2f8b…` and `sha256-fa9f700146a9…`) also
pinned each pack as they fetched it, so the apply skipped a package
that had been applied before and left it unlinked: on a machine with
either, take the newer `oath` by hand first, as above, then update.

An `oath` from before R42 (`pkg:oath` `sha256-618db152642b…` and older)
does not make the new packages the others need, nor hand `/lib`'s names
from one package to another, so its apply refuses:
nothing converges, but the specs it set stay as drift, and the next
update sets them again. An `oath` from before 2026-10-30 (`pkg:oath`
`sha256-92735e24e583…` and older) does not build a recipe the owner
built, so a Steam pinned to the old 32-bit Mesa makes its apply refuse
the same way (canto, 2026-10-29), and it restarts the desk under the
person at it, `--reboot` or not. Take the newer `oath` (desk output
`fed6fb141a5e`'s) first, then update:

```sh
sudo oath set pkg:oath hash=sha256-a937565c5f8a5a210f233f9ec58c56ec1ed844e93fc5847fb2edddedbdf890a5 && sudo oath apply pkg:oath
oath update && sudo oath update --confirm --reboot
```

A machine from before R38 has no `oath update` yet. Give it the newer
`oath` first (the pin is `pkg:oath`'s `hash` in the output's seed,
`oath get pkg:oath` on any newer machine), then the profile it follows
and the key it trusts:

```sh
sudo oath set pkg:oath hash=sha256-… && sudo oath apply pkg:oath
sudo oath set host:local profile=desk \
  publishers='["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH+YKD3yXYsGJMxwpBIWSYP7VuQvjhzGG+LWvYhKTESD oath@buccina"]'
sudo oath apply host:local
sudo oath update --confirm --reboot
```

To try an output first, `cargo make update --qemu <older output>`
installs the older one in QEMU and runs the same update from a local
origin on the build host.

If the new boot fails, pick the archived boot in the firmware menu: it
is the old kernel with the old userspace.

`cargo make esp` does steps 2 and 6 on the Oath machine itself, for an
ESP you mount there. See [Generations](generations.md) for why it must
not be pointed at the build host's own ESP.
