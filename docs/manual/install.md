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
nothing changes the firmware's boot order.

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
on probation. With `--confirm`, it:

1. snapshots `@` as `@boot-<n>` before changing anything;
2. writes the output's accounts (`/etc/passwd`, `group`, `shadow`) and
   `/var/empty`, since services that restart next drop to `void` (R36);
3. sets each package the machine has to the output's pin and needs,
   makes each package the output fixes that the machine lacks (the
   image's files need it: `/etc/passwd` names `pkg:zsh`'s shell), and
   runs `oath apply` once, which fetches each pack from the origins
   (`oath undo` takes it back but keeps a fixed package; other objects
   the output's seed has and the machine does not, a new base service
   say, are listed, not created: add them with `oath new`). A package
   the profile pins from elsewhere (R28) keeps the machine's own pin,
   and only its needs move to the output's hashes (a need on another
   such package the machine keeps stays at the machine's pin); so does
   a package the output does not carry (`pkg:dropbear` after R36, one
   its owner added). A recipe the owner built is listed with the
   commands that build it here. Each service whose program moved
   restarts; when sshd's did, the apply is on probation, and the
   update waits until you open a **new** SSH session and run `sudo oath
   keep` there;
4. replaces `/lib/oath`, the image's static `/etc` files, `/sbin/init`
   and `/bin/sudo`;
5. archives the ESP's kernel and initrd as boot `<n>` (with the
   snapshot from step 1) and writes the new kernel, initrd and
   `oath-efi`, keeping five archives (`--esp`, default partition 1 of
   the root disk);
6. with `--reboot`, reboots. `oath update` afterwards says whether
   PID 1 is the output's.

A machine that still runs dropbear as `svc:sshd` (from before R36)
moves to OpenSSH after it has booted the new PID 1, which writes
`/etc/ssh/sshd_config` and carries dropbear's host key over: run `sudo
oath update --confirm` again, then `sudo oath keep` from a new SSH
session. If none gets in, PID 1 undoes it and dropbear serves again.

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

`cargo make esp` does steps 1 and 5 on the Oath machine itself, for an
ESP you mount there. See [Generations](generations.md) for why it must
not be pointed at the build host's own ESP.
