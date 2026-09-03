# Install

Wipe a named disk and copy the packed Oath tree onto it.

```sh
cargo make build
cargo make install --target user@host --disk /dev/sda --confirm --hostname canto
```

`--confirm` is required. `--disk` is the **whole disk** (GPT). The live
OS on that host must be reachable over SSH (`sudo -n` if you are not
root).

What it does:

1. Boot an **installer ramdisk** on the target (`oath.install=1`,
   dropbear, no `switch_root`). On EFI machines this is a one-shot
   loader entry or a USB stick (`--usb`). kexec is tried when there
   is no EFI loader; it did not bring canto’s Broadcom `tg3` up.
2. Format GPT: 512M ESP + rest btrfs subvolume `@`.
3. Copy the packed root, write `oath-efi` as `BOOTX64.EFI` (white
   mark on black at native GOP, then the kernel), keep systemd-boot
   at `EFI/systemd/` as fallback, kernel, initrd,
   `loader/entries/oath.conf`. Apple/OEM firmware splash is not
   ours to paint. The USB installer still uses systemd-boot + tty0.
4. Set `host:local` hostname, `net:net0` dhcp, owner SSH pubkeys.
5. Reboot. Courage is **SSH as `home`** with those keys, then
   `oath ls`. (`sudo` has no password.)

QEMU rehearsal (no real host):

```sh
cargo make install --qemu --disk /dev/vda --confirm
```

USB installer stick (must be removable USB):

```sh
cargo make install --usb --disk /dev/sdd --confirm
```

Hold Option on a Mac, boot the EFI entry. The ramdisk gets a shell on
`tty0` and DHCP on the NIC that has carrier. Then run the `--target`
command from the build machine; it sees `/run/oath-install/ready` and
skips the enter-installer step.

QEMU sets `SOLA_OUTPUT_PICK=preferred` so virtio-gpu stays 1280×800.
Metal unsets that so sola-river can match the panel. Canto’s
graphical stack is **off** until DRM-as-`home` is fixed; SSH is the
courage test.
