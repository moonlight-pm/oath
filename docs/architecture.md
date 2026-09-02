# Architecture

**Role:** as-built system map (what the code and runtime look like **now**).
**Not** the place for multi-feature roadmaps or session priority.

| Concern | Document |
|---------|----------|
| Capability maturity | [capabilities.md](capabilities.md) |
| Target design freezes | [specs/](specs/) |
| Session priority + dogfood | Root [CURRENT.md](../CURRENT.md) |
| How docs fit together | [progress-model.md](progress-model.md) |
| Product docs | [manual/](manual/) — current operator manual |

---

## As-built (2026-09-02)

QEMU x86_64 appliance. Serial, SSH, and (if DISPLAY) a gtk window.

```
QEMU -kernel bzImage -initrd initrd.gz -netdev user -device virtio-net-pci
-device virtio-gpu-pci,xres=1280,yres=800 -display gtk,zoom-to-fit=off (or none) -drive virtio qcow2
  kernel (borrowed Linux 6.12) + initramfs
    /init = oath-init
    loads virtio_blk, btrfs, virtio-gpu, evdev, virtio_input, virtio_net, …
    mounts /dev/vda subvol=@ , chroot
    mounts subvolid=0 at /oath/run/fs
  disk (btrfs)
    @            live root
    @gen-N       readonly sibling generations
    /lib/oath/init         PID 1 after pivot
    /lib/oath/serial-login
    /lib/oath/sudo         setuid; /bin/sudo
    /bin/*                 symlink farm into /oath/store/pkg/<name>/bin/
    /home                  seat home (Unix user `home`, uid 1000)
    /oath/                 catalog
    /oath/store/pkg/{busybox,btrfs,oath,dropbear,glibc,river,sola,hello,fetchme}/
    net0               virtio-net (QEMU user or OATH_BRIDGE)
    /dev/dri/card0     virtio-gpu (dev:card0)
    /dev/input/event*  virtio keyboard + mouse (dev:kbd0, dev:mouse0)
    /oath/ssh/         dropbear host keys (generated)
    dropbear           svc:sshd, keys from ssh:local
    seatd              svc:seatd (DRM seat)
    river              svc:river as `home` (glibc, pixman, libudev-zero, socket /run/user/1000;
                       hardware cursors unless a DRM card is virtio)
    sola-bus/call      svc:sola-bus / svc:sola-call as `home` (sockets /run/user/1000)
    sola-river         svc:sola-river (bridge, not the compositor)
    sola-shell         svc:sola-shell (iced menubar; wgpu/gl; llvmpipe forced
                       only on virtio KMS; McMojave)
    sola-session       svc:sola-session (LaunchApp; direct spawn)
    sola-terminal      /bin/sola-terminal (kit app in pkg:sola; tmux helper)
    sola-browser       /bin/sola-browser (kit app in pkg:sola; CEF under cef/)
    sola-workspaces    /bin/sola-workspaces (kit app in pkg:sola; tmux sola-ws)
    solactl            /bin/solactl (call-plane CLI in pkg:sola)
    pkg:sola fonts     SF Pro Text + Iosevka Term Slab (Inter / JetBrains Mono fallbacks)
    /sbin/init -> ../lib/oath/init
```

PID 1: mount proc/sys/dev/pts, tmpfs `/tmp` `/dev/shm` `/run`, cgroup2;
hostname from `host:local`; **converge** `net:net0`, `dev:*`,
`ssh:local`; then `svc:*`. Socket `/oath/run/init.sock`. Seeded
services: `svc:serial`, `svc:hold`, `svc:sshd`, `svc:seatd`, `svc:river`,
`svc:sola-bus`, `svc:sola-call`, `svc:sola-river`, `svc:sola-shell`,
`svc:sola-session`.

`oath apply` snapshots live `@` to sibling `@gen-N` under `/oath/run/fs`
(btrfs top-level). Undo restores catalog documents (including `store/`)
from that generation, not `/oath/run`. Fallback: copy the catalog tree
when the top-level is not mounted.

Telemetry: guest lines `oath-tel {json}` on stderr and `/oath/log/*.jsonl`.
`oath apply` on `pkg:*` creates or removes `/bin` symlinks into
`/oath/store/pkg/<name>/bin/`. Undo restores `store/` with the catalog
then converges links.

Host runs live under `build/runs/<id>/` (`cargo make run` / `up` /
`start` / `probe`). `cargo make ssh` is hostfwd 2222.

**Metal install (T27):** `cargo make install --target user@host --disk
/dev/sda --confirm` (`--qemu` OVMF rehearsal; `--usb --disk /dev/sdd`
writes an EFI installer stick). Installer ramdisk: `oath.install=1`,
dropbear, no `switch_root`. Format GPT ESP + btrfs `@`. Copy packed
tree. Boot graphics is layered, not one path: EFI GOP splash
(`oath-efi` as `BOOTX64.EFI`) when firmware has GOP; PID 1 defers KMS
drivers that would kick a live firmware framebuffer (amdgpu/i915/…,
not virtio-gpu) until just before River; River starts black until
Sola paints. USB installer still systemd-boot + tty0. QEMU `run`
is still `-kernel`. `loader/entries/oath.conf`.
Canto: two Broadcom `tg3` ports; live cable is MAC
`00:3e:e1:cb:06:08` (renamed `net0`). kexec left that NIC down; EFI
oneshot / USB installer is the working entry. After boot, PID 1 waits
for carrier then dhcp. Dual Pitcairn amdgpu (`si_support=1`);
`/lib/oath/run-compositor` binds River to the connected DRM card. The
graphical stack runs as Unix user `home`. SSH is `home`; serial is root.
sola-river picks the mode matching physical mm.

Workspace crates: `oath-core`, `oath`, `oath-init`, `oath-efi` (UEFI
splash), `oath-make` (host build CLI: `cargo make`). Artifacts in
`build/` (gitignored).
Source forks under `forks/`: `river`, `wlroots`, `sola` (`oath-sola`).

**Target:**
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md) ·
[specs/2026-08-28-packages.md](specs/2026-08-28-packages.md) ·
[specs/2026-08-29-pkg-base.md](specs/2026-08-29-pkg-base.md) ·
[specs/2026-08-29-net.md](specs/2026-08-29-net.md) ·
[specs/2026-08-30-ssh-and-dhcp.md](specs/2026-08-30-ssh-and-dhcp.md) ·
[specs/2026-08-30-devices.md](specs/2026-08-30-devices.md) ·
[specs/2026-08-30-display.md](specs/2026-08-30-display.md) ·
[specs/2026-08-30-pkg-hosting.md](specs/2026-08-30-pkg-hosting.md) ·
[specs/2026-08-30-sola.md](specs/2026-08-30-sola.md) ·
[specs/2026-08-30-libinput.md](specs/2026-08-30-libinput.md) ·
[specs/2026-08-30-oath-sola.md](specs/2026-08-30-oath-sola.md) ·
[specs/2026-08-31-sola-dev.md](specs/2026-08-31-sola-dev.md) ·
[specs/2026-08-31-sola-session.md](specs/2026-08-31-sola-session.md) ·
[specs/2026-08-31-sola-terminal.md](specs/2026-08-31-sola-terminal.md) ·
[specs/2026-08-31-metal-canto.md](specs/2026-08-31-metal-canto.md) ·
[specs/2026-09-01-sola-browser.md](specs/2026-09-01-sola-browser.md) ·
[specs/2026-09-02-sola-workspaces.md](specs/2026-09-02-sola-workspaces.md)
