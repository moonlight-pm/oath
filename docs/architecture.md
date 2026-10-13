# Architecture (as built)

What runs and where things live today. Target design is in
[specs/](specs/README.md). Update this file when a process, path or
tree changes.

**As of:** 2026-10-13, base profile, booted in QEMU.

## Boot

```
firmware ── ESP: BOOTX64.EFI (systemd-boot today; oath-efi is the menu/mark)
            loader/entries/oath.conf  + oath-N.conf (last 5 boots)
            vmlinuz (own build, 7.3.0-rc1) + initrd.gz
  kernel cmdline: oath.root=<dev> oath.subvol=@ console=ttyS0 …
  initrd /init = oath-init (PID 1, static musl)
    mount /proc /sys /dev /dev/pts; tmpfs /tmp /run /dev/shm; cgroup2
    insmod the packed module list (net and disk first, KMS and sound later)
    mount <root> subvol=@ at /newroot, bind initrd modules+firmware, chroot
    mount btrfs subvolid=0 at /oath/run/fs (generations are siblings of @)
    apply host, net (udhcpc for dhcp), dev, ssh from the catalog
    start svc:* in `wants` order; listen on /oath/run/init.sock; reap
```

QEMU (`cargo make run|up|start`) boots the same kernel and initrd with
`-kernel` against `build/oath.qcow2` and forwards SSH to 127.0.0.1:2222.

## Disk

```
GPT  1: ESP (vfat, 512M)     2: btrfs "oath"
btrfs
  @           live root
  @gen-N      read-only snapshot taken by each apply (undo target)
  @boot-N     snapshot taken when `cargo make esp` archives a boot
```

## Root tree

```
/sbin/init → /lib/oath/init        same ELF as the initrd's /init
/lib/oath/  serial-login, sudo (setuid), udhcpc.script, backup-send, backup-daily
/bin/       symlink farm → /oath/store/pkg/<name>/live/bin/<x>
/home       the `home` account (uid 1); SSH logs in here
/etc        written by PID 1 from host:local (hostname, hosts, profile, shells)
/oath/      the catalog
  INDEX.md                 generated; what an agent reads first
  schema/<kind>.json|.md   per-kind schema and prose
  objects/<kind>/<name>/   desired.json actual.json meta.json [applied.json]
  log/apply.jsonl          one line per successful apply or undo
  store/pkg/<name>/<hash>/ package trees; `live` → the linked hash
  store/plan/<name>/<hash>/plan.plan
  store/adv/<name>/<hash>.json   advisories
  ssh/host_ed25519         dropbear host key
  run/                     init.sock, fs/ (btrfs top level)
```

## Processes (base profile)

| Process | From | Runs as |
|---------|------|---------|
| `oath-init` | initrd `/init` | PID 1, root |
| `svc:sshd` | `/bin/dropbear -F -E -s -w` | root (logins are `home`; root refused) |
| `svc:serial` | `/lib/oath/serial-login` on ttyS0 | root (break-glass) |
| `svc:hold` | `/bin/sleep` | root (canary service) |
| `udhcpc` | run by PID 1 during net apply | root, exits after the lease |

## Host side (novus)

```
cargo make build [--profile base]
  cargo build oath, oath-init (musl), oath-efi (uefi)
  kernel + modules from build/linux (image/build-linux.sh), vermagic checked
  busybox, btrfs, dropbear, … from image/tools.nix (temporary, phase 5)
  build/stage/            the root tree; `oath seed --profile` writes /oath
  .cache/oath/store/      host copy of every packed tree, by hash
  build/oath.qcow2        btrfs @ with the stage
  build/initrd.gz, build/initrd-install.gz (installer ramdisk: dropbear,
                          sgdisk, mkfs, oath)
cargo make check
  one QEMU per case in check/cases.toml (4 at a time), SSH as home with
  build/check/key; serves pkg:fetchme on 127.0.0.1:18765; a clean run
  writes build/check-pass (kernel, initrd, image sha256)
cargo make install --target … --disk … --overlay hosts/<name>.json --confirm
  refuses an image without a matching build/check-pass
  kexec or EFI-boot the installer, format, copy the stage, write the
  overlay with `oath seed --overlay`, write the ESP, reboot
```
