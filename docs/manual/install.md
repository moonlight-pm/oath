# Install

Wipe a named disk and copy the packed Oath tree onto it.

```sh
cargo make build
cargo make install --target user@host --disk /dev/sda --confirm \
  --hostname canto --overlay hosts/canto.json
```

`--confirm` is required. `--disk` is the **whole disk** (GPT). `--target`
is `[user@]host[:port]`; no user means `root`. The live OS on that host
must be reachable over SSH as root, or as a user with `sudo -n`. The
installer ramdisk's sshd and the installed system's listen on 22, so a
port names a forward to the machine's 22 (QEMU's, a NAT's), and every
stage is reached there: the live OS, the ramdisk, the pinned key and the
new system. When the ramdisk does not answer at a port other than 22 (a
live OS whose own sshd listens there), `install` stops before anything is
wiped and says to run it again with `--target root@<host>`. Every command
is fed to the target's `/bin/sh` on stdin and files are written with
`dd`, so the user's login shell does not matter.

## What it does

1. Picks a build output: `--output <id>`, else the newest one that
   passed `cargo make check`, or with `--unchecked` or `--qemu` the
   latest. Its `check-pass` must match the kernel, initrd and image,
   a named output's too, unless `--unchecked` or `--qemu` is given.
   Everything below comes from that one output; the build host's keys
   are baked into a copy of its installer ramdisk under
   `build/runs/<time>-install/`, never into the output.
   Reads and checks `--overlay` against a scratch catalog of the output's
   own profile (its manifest's, the catalog step 5 patches) before
   anything is wiped, so a patch on an object that profile does not have
   must be valid alone (a service needs its `exec`). Refuses a `--target`
   that is the machine it runs on (the same `boot_id`).
2. Boots an **installer ramdisk** on the target (`oath.install=1`,
   OpenSSH's sshd, the build host's keys for root). On EFI machines whose
   ESP has `loader/entries` this replaces that ESP's `oath.conf` and the
   loader's default, so the machine boots the installer until step 3 wipes
   the ESP; kexec is the fallback (Oath's own static `kexec`, copied to
   the target, so the live OS needs none).
3. Partitions GPT with the ramdisk's native tools (`wipefs`, `sfdisk`):
   a 512 MiB ESP (`mkfs.fat`), then btrfs (`mkfs.btrfs`) with subvolume
   `@` and a new UUID, which the boot entry names the root by
   (`oath.root=UUID=…`): the disk's node changes with the order disks
   are found in. There is no `@home`: `/home` is part of `@`, in every
   generation and boot snapshot, unless `host:local.mounts` names it.
   `oath install`, below, makes an `@home`.
4. Copies the packed root tree (GNU `tar`).
5. Merges the catalog overlay with the ramdisk's `oath`
   (`oath --root /mnt/oath seed --overlay`): `host:local.hostname`
   (default `oath`), the build host's public keys as `ssh:local`, then
   your `--overlay` on top. Makes the new system's SSH host key
   (`/oath/ssh/ssh_host_ed25519_key`) and pins it in `build/known_hosts`.
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
      "exec": ["/lib/oath/backup-daily", "nas:/export/backup"],
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

`--oath-install` with `--qemu` rehearses `oath install` (below): it
installs the output on a stick in QEMU, boots it beside a blank disk
and a disk that must not change, runs `oath install <serial> --confirm`,
and boots the firmware's first entry. `--disk` is required and must be a
`/dev/` node, but its value is not used.

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
sudo poweroff                         # then remove the stick and start the machine
```

The disk is a node (`/dev/sdb`, `sdb`) or a serial; `sudo oath install`
also shows each filesystem's type and label. Node names follow the
order the kernel found the disks in and can differ from another
system's, so check the model and serial the preview repeats.

`--confirm` wipes that disk: a GPT with a 512 MiB ESP and btrfs for the
rest, with `@` (the system) and `@home` (mounted at `/home`, outside
every generation and boot snapshot; the first boot adds `@work`, at `/work`,
likewise). It copies the system the stick
runs: its packages (NVIDIA's driver too, if the stick built it), its
settings (hostname, overlay, SSH keys it trusts) and its release, at
generation 0, with an empty home, a new SSH host key (printed at the
end), and none of the stick's history, password, pairings or machine id:
its first boot makes its own. It writes
the ESP, adds `Oath` to the firmware's boot list first and removes the
other systems' entries; the firmware's own (USB, network, setup) stay,
and the only other firmware variable it touches is a `BootNext` that
names a removed entry, which it deletes.

Remove the stick only once the machine is off: the system runs from it
until then, and `reboot` with the stick gone finds no programs. Some
firmware keeps its own order or makes entries again, such as AMI's
`UEFI OS` entry for another disk's fallback loader. If the machine
starts something else, move `Oath` first in the firmware's setup.

It checks no backups and reads no other disk past each filesystem's
type and label: knowing what is on them is yours. It refuses the disk
the stick runs from, a disk with something mounted, pending changes (a
cell whose tree is missing or damaged among them: the hint names `sudo
oath store cell:<name> <dir|file.tar>`), an apply on probation and an
archived boot. Agents run the same over SSH (`--json` for each
step).

## USB installer stick

```sh
cargo make install --usb --disk /dev/sdd --confirm
```

The disk must be a removable USB device. Boot the stick's EFI entry
(hold Option on a Mac). The ramdisk gets a shell on `tty0` and DHCP on
the NIC with carrier. Then run the `--target root@<host>` command from
the build host (root is the ramdisk's only login); it sees
`/run/oath-install/ready` and skips step 2.

On a network with no DHCP, add `oath.ip=<a.b.c.d/n>` and
`oath.gw=<gateway>` to the `options` line of the stick's
`loader/entries/oath-install.conf`: the ramdisk sets that address on
the NIC and runs no DHCP. A ramdisk left with no address writes
`oath-kexec.log` (its command line, NICs, addresses, PCI devices and
`dmesg`) to the first of `/dev/sda1`, `/dev/nvme0n1p1`, `/dev/vda1` and
`/dev/sdb1` that mounts as FAT, if any, then tries the network once
more and starts `sshd` anyway.

## A disk image to download

```sh
cargo make image                      # the newest checked output
cargo make image --output <id> --publish
```

Writes a checked output as a disk image anyone can write to a USB stick
(R86): `build/out/<id>/oath-<profile>-<id>.img.xz`, with its `.sha256`
and its `.sig`, signed by the build host's key in the namespace
`oath-image`. It is the system `--local` writes to a stick, on an 8 GiB
disk, with no SSH key in `ssh:local` and no host key: each machine that
boots it makes its own. A copy of the image boots once in QEMU through
OVMF, and its serial console must answer `oath ls`; the image itself is
never booted. An image already made is reused. `--publish` puts the
three files on the default origin under `image/`, once per output, and
only after that output's release is there, so a machine installed from
the image can update.

A person writes it to a stick of 16 GB or more and checks it first:

```sh
sha256sum -c oath-desk-<id>.img.xz.sha256
echo 'oath@buccina ssh-ed25519 AAAA…' > allowed_signers   # the release key
ssh-keygen -Y verify -f allowed_signers -I oath@buccina -n oath-image \
  -s oath-desk-<id>.img.xz.sig < oath-desk-<id>.img.xz
xzcat oath-desk-<id>.img.xz | sudo dd of=/dev/sdX bs=4M oflag=direct status=progress
```

Then boots it from the firmware's boot menu (Secure Boot off) and runs
`oath install` ([above](#from-a-stick-on-the-machine-oath-install)).
The installed machine has hostname `oath`, no password and no SSH key.
Published images are at `https://store.oath.moonlight.pm/image/`, and
oath.moonlight.pm links the newest. A `desk` image carries NVIDIA's
driver (R89, [NVIDIA](packages.md#nvidia)). On a card no driver here
takes, PID 1 gives the firmware's framebuffer to `efidrm` once the boot
ends and the compositor starts on it, but Sola's apps need a GPU of
their own: the screen is black but for the pointer.

## Update an installed machine

Each machine updates itself from its origins (R38). The build host
publishes the release of its newest checked output (`cargo make publish`;
`--output <id>` names another it checked, [Packages](packages.md)); on
the machine:

```sh
oath update                           # the newest output for this machine, and what moves
sudo oath update --confirm            # move to it; PID 1 re-executes into its binary
sudo oath update --confirm --reboot   # move to it, then boot it
```

It takes the newest release for `host:local.profile` from the first of
`host:local.origins` that serves one signed by a key in
`host:local.publishers` with no `names=` list (the `base` profile trusts
buccina's), and refuses one older than the release the machine runs
(`/oath/output.json`, written by the install too), or one it cannot
order against it, unless `--output <id>` names it. It also refuses
pending changes (a cell whose tree is missing or damaged is one:
[Cells](cells.md#cellname-one-projects-own-software)) and an apply on
probation, and a set of moves that would leave a package needing
another at a hash it will not have (the check `oath apply` makes, made
first, so the preview says it too). Apply and undo do not repair such a
cell, so its hint names the store: `oath diff; a cell comes from no
origin: sudo oath store cell:<name> <dir|file.tar>`, ending `; apply or
undo the rest first` when other changes are pending too.

Every package `cargo make publish` publishes has a signed head, except
`linux`, `oath` and `firmware`, which move with the release; a recipe
has none. Each `oath update` reads the head of every package the machine
has and every one the release makes (new to the profile, or one the
output fixes), and of what those heads need, and takes the newest a key
in `host:local.publishers` may sign for that name ([Packages](packages.md)).
A head wins over the release's pin, and the preview says so:
`pkg:certs sha256-… -> sha256-… (its head, signed by oath@buccina)`; a
package the release makes is made at its head, with what that head
needs. A head of the pack the machine runs moves only what differs, its
needs as a rule: `pkg:curl sha256-… (its head, signed by oath@buccina;
its needs move)`. A head no trusted key may sign is refused, one line
each, and its package follows the release: `pkg:sola's head on … is
refused: security: signed by a key this machine does not trust`. A head
never goes back: one older than the head the machine took
(`/oath/heads.json`) waits, of a package it has, of one the release
makes, or of one it took and removed since that a head here needs
again, and what the update would move or make that needs one it lacks
waits with it: `pkg:libcap sha256-… waits: it (committed …) is older than the
head this machine took, sha256-… (signed by oath@buccina, committed …)`,
`pkg:bind sha256-… waits: pkg:bind needs pkg:libcap, which waits`. One
the machine has reads `stays at sha256-…; sha256-… waits: …`. The times
are the heads' commits when both name one and they differ, else when
each was published.
`--output
<older>` moves the machine to that release except each package with a
head the machine trusts, which goes to its head, never to that release's
pin. `oath undo` takes back the apply that moved a package to its head;
the next update moves it there again unless `--keep pkg:<name>` names
it. On the newest release already, or with `host:local.profile` empty,
the update moves heads only: no boot file changes, and `--reboot`
reboots only if a package moves.

The preview names each **overlay**, a package the machine runs at a pin
other than the head it took of it or, with none taken, the pin the
release it runs set (R47, [Packages](packages.md)): ``pkg:certs is
sha256-…, not its head: the update replaces it with sha256-… (signed by
oath@buccina); `--keep pkg:certs` keeps it``, or, for a package with no
head it trusts, ``pkg:sola is sessions-anywhere@0d99e340, not the
release's: the update replaces it with sha256-… (master@…); `--keep
pkg:sola` keeps it``. The update replaces it, except that when it moves
heads only, an overlay of a package with no head it trusts stays, and
the preview does not name it.
`--keep pkg:<name>` (repeat for more) keeps the machine's pin of any
package the update would move, and only its needs move: `pkg:certs stays
at sha256-…; sha256-… waits: --keep`. Each `--keep` names a package
(`pkg:<name>` or `<name>`) or a service (`svc:<name>`) the machine has;
any other is refused: `--keep svc:bogus: this machine has no svc:bogus`.

With `--confirm`, it:

1. fetches every pack it will pin that the store lacks, and builds each
   recipe the owner built (Steam) from the output's
   plan, before it changes anything: a fetch or build that fails (a
   network error is tried twice more), or a build that is not the
   output's pin, leaves the machine as it was, and running it again
   fetches the rest. Then no two packages as the moves leave them may
   ship one name in `/bin` or `/lib` (a package kept with `--keep` that
   the output split in two), or it stops there;
2. has PID 1 re-execute into the output's binary (below; not with
   `--reboot`), then snapshots `@` as `@boot-<n>`;
3. writes the output's accounts (`/etc/passwd`, `group`, `shadow`) and
   `/var/empty`, since services that restart next drop to `void` (R36);
4. sets each package the machine has to the output's pin, needs and
   `source`, or to its head when a key the machine trusts for it signed
   one ([Packages](packages.md)); makes each package and service new to
   the profile since the release the machine runs (its record lists
   that release's objects, `seed`; a record from before R47 tells only for the
   packages pinned from elsewhere, `pinned`) and each package the output
   fixes (the image's files need it: `/etc/passwd` names `pkg:zsh`'s
   shell), though nothing here needs it, and each one a package it has
   present needs, down their needs (R41's `pkg:libglvnd`), unless what
   needs it waits; the preview says why it makes each (`pkg:dosfstools
   sha256-… is made (new to the profile)`, `(the output fixes it)`, `(a
   package here needs it)`); files each plan whose pin the output moves and
   each new to the profile; and runs
   `oath apply` once (a name in `/bin` or `/lib` that moves from one
   package to another, `libEGL.so.1` there, moves in it whichever
   converges first). `oath undo` takes it back but keeps a fixed
   package. An object the release the machine runs had and the machine
   lacks was its owner's to remove and is not made again; one new to the
   profile that the machine has absent (`oath build` leaves a product it
   made so) is made present, never the other way, at its head when it
   has one the machine trusts: `pkg:dosfstools sha256-… -> sha256-… (its
   head, signed by oath@buccina; made present: new to the profile)`, or,
   when that head is the pin it has, `pkg:dosfstools sha256-… (its head,
   signed by oath@buccina; made present: new to the profile)`. Any other
   the output's seed has is listed, for `oath new`. A package the release
   the machine runs had and this one does not is removed, unless `--keep`
   names it, it has a head, or something that stays needs it (R62); one its
   owner added keeps its pin, or moves to its head when it has one. A
   service that release had and this one does not is removed too, unless
   `--keep svc:<name>` names it.
   A recipe the owner built moves to
   the pin step 1 built. Each running service whose program moved
   restarts on it, except one with a `seat` (the desk's and the
   console's, R42), which keeps its program until it next starts: the
   desk takes its new programs at a reboot, or at Exit Sola and `oath
   desk`. With `--reboot` no service restarts, and the boot starts each
   on its new program. When sshd's program moved, the apply is on
   probation: the services restart then (the seat's still keep theirs),
   and the update waits until you open a **new** SSH session and run
   `sudo oath keep` there. If none does in 180 s, PID 1 undoes the
   apply and the update stops; run it again and keep it from a new
   session;
5. replaces `/lib/oath`, the image's static `/etc` files, `/sbin/init`
   and `/bin/sudo`;
6. archives the ESP's kernel and initrd as boot `<n>` (with the
   snapshot from step 2) and writes the new kernel, initrd and
   `oath-efi`, keeping five archives (`--esp`, default partition 1 of
   the root disk);
7. with `--reboot`, reboots; without it, says what waits for a boot: a
   new kernel or firmware, a PID 1 that could not re-execute, and, when
   PID 1 re-executed, what it does only at boot (`mounts, modules,
   sysctls, device modes, /etc but its machine-id`). `sudo oath update`
   afterwards says whether PID 1 is the output's, and `sudo oath update --confirm`
   re-executes it when it is not. An `oath` from before R47 that made the
   update wrote the record without the release's commit, pins and seed;
   `oath update` says so, and `sudo oath update --confirm` (the same
   release) writes the record as signed. Objects that release's seed
   has and the machine lacks are, from then on, the owner's to add
   (`oath new`).

**A new PID 1 without a reboot (R73).** When the output's PID 1 is not
the one running, an update without `--reboot` asks PID 1 to re-execute
into the output's binary before it applies anything, so the new PID 1
is the one that restarts what moved. PID 1 checks the binary's hash,
lets it read a copy of its state first, and refuses one that cannot;
every service keeps its process. The update prints `PID 1 runs …`, and
`--json` says `"pid1": "re-executed"`. If the update stops after that,
PID 1 goes back to the binary it ran. A new kernel or new firmware takes a
reboot, and so does what PID 1 does only at boot (mounts, modules,
sysctls, device modes, `/etc`), except `/etc/machine-id`: the PID 1 that
takes over writes it from `/oath/machine-id`, and makes that id when the
machine has none, before the update goes on. A PID 1 from before R73 is
not asked: its first update to one takes a reboot.

A machine whose `svc:sshd` runs dropbear moves to OpenSSH (R36) after
it has booted the new PID 1, which writes `/etc/ssh/sshd_config` and
carries dropbear's host key over: run `sudo oath update --confirm`
again, then `sudo oath keep` from a new SSH session. If none gets in,
PID 1 undoes it and dropbear serves again.

A machine whose `oath` is too old to make the update takes the newer
`oath` by hand first. The pin is `pkg:oath`'s `hash` in the output's
seed (`oath get pkg:oath` on any newer machine):

```sh
sudo oath set pkg:oath hash=sha256-… && sudo oath apply pkg:oath
```

One with no `oath update` also needs the profile it follows and the key
it trusts:

```sh
sudo oath set host:local profile=desk \
  publishers='["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH+YKD3yXYsGJMxwpBIWSYP7VuQvjhzGG+LWvYhKTESD oath@buccina"]'
sudo oath apply host:local
```

Then update:

```sh
oath update && sudo oath update --confirm --reboot
```

To try an output first, `cargo make update --qemu <older output>`
installs the older one in QEMU and runs the same update from a local
origin on the build host, signed with the build host's key
([Packages](packages.md)); that origin serves no heads, so every package
follows the output. It updates to the newest output `check`
passed; `--output <id>` names another, and `--unchecked` takes one that
has not passed (the latest, without `--output`). `--oath-first` moves
`pkg:oath` to the output's first. `--build plan:<name>` (repeatable)
builds that recipe on the older machine and makes its package present,
as its owner does. `--live` updates without `--reboot` first and requires
PID 1 to re-execute into the output's on the same boot and every service
whose package did not move to keep its process, then reboots and checks
as usual.

If the new boot fails, pick the archived boot in `oath-efi`'s boot menu
([Generations](generations.md)): it is the old kernel with the old
userspace.

`cargo make esp --esp <node> --confirm` does step 6 and the snapshot of
step 2 on the Oath machine itself: it mounts the node you name there, and
refuses a machine that is not Oath. The boot entries it writes name the
root the running kernel booted (its `oath.root=`, the UUID the install
wrote), or `--root` (a node or `UUID=…`), and keep the kernel options the
current entry added, as `oath update` does; a kernel booted with no
`oath.root=` needs `--root`. It writes the default `loader/loader.conf`
(a 5 s menu) over the ESP's. See [Generations](generations.md).
