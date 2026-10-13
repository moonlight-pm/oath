# Install

Wipe a named disk and copy the packed Oath tree onto it.

```sh
cargo make build
cargo make install --target user@host --disk /dev/sda --confirm \
  --hostname canto --overlay hosts/canto.json
```

`--confirm` is required. `--disk` is the **whole disk** (GPT). The live
OS on that host must be reachable over SSH as root, or as a user with
`sudo -n`.

## What it does

1. Refuses an image that has not passed `cargo make check` (the stamp
   in `build/check-pass` must match the kernel, initrd and image).
   `--unchecked` skips this; `--qemu` rehearsals are not gated.
   Reads and checks `--overlay` against a scratch catalog, before
   anything is wiped.
2. Boots an **installer ramdisk** on the target (`oath.install=1`,
   dropbear, the build host's keys for root). On EFI machines this is
   a one-shot loader entry; kexec is the fallback.
3. Partitions GPT: a 512 MiB ESP, then btrfs with subvolume `@`.
4. Copies the packed root tree.
5. Merges the catalog overlay with the ramdisk's `oath`
   (`oath --root /mnt/oath seed --overlay`): `host:local.hostname`
   (default `oath`), the build host's public keys as `ssh:local`, then
   your `--overlay` on top.
6. Writes the ESP: `oath-efi` as `EFI/BOOT/BOOTX64.EFI`, systemd-boot
   at `EFI/systemd/` as a fallback, `vmlinuz`, `initrd.gz`,
   `loader/entries/oath.conf`, and `loader/oath-boots`. The kernel and
   the initrd modules are the same checked build.
7. Reboots and waits for SSH as `home`, then runs `oath ls`.

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

## Rehearse in QEMU

```sh
cargo make install --qemu --disk /dev/vda --confirm
```

That runs the same steps against a fresh qcow2 in QEMU with OVMF, then
boots it through the EFI loader.

## USB installer stick

```sh
cargo make install --usb --disk /dev/sdd --confirm
```

The disk must be a removable USB device. Boot the stick's EFI entry
(hold Option on a Mac). The ramdisk gets a shell on `tty0` and DHCP on
the NIC with carrier. Then run the `--target` command from the build
host; it sees `/run/oath-install/ready` and skips step 2.

## Later kernel and initrd updates

`cargo make esp` rotates a new kernel and initrd onto an existing ESP
without wiping. See [Generations](generations.md) for what it does and
why it must not be pointed at the build host's own ESP.
