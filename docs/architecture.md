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

## As-built (2026-08-31)

QEMU x86_64 appliance. Serial, SSH, and (if DISPLAY) a gtk window.

```
QEMU -kernel bzImage -initrd initrd.gz -netdev user -device virtio-net-pci
-device virtio-gpu-pci -display gtk (or none) -drive virtio qcow2
  kernel (borrowed Linux 6.12) + initramfs
    /init = oath-init
    loads virtio_blk, btrfs, virtio-gpu, evdev, virtio_input, virtio_net, …
    mounts /dev/vda subvol=@ , chroot
    mounts subvolid=0 at /oath/run/fs
  disk (btrfs)
    @            live root
    @gen-N       readonly sibling generations
    /usr/lib/oath/init     PID 1 after pivot
    /usr/lib/oath/serial-login
    /bin/*                 symlink farm into /oath/store/pkg/<name>/bin/
    /oath/                 catalog
    /oath/store/pkg/{busybox,btrfs,oath,dropbear,glibc,river,sola,hello,fetchme}/
    net0               virtio-net (QEMU user or OATH_BRIDGE)
    /dev/dri/card0     virtio-gpu (dev:card0)
    /dev/input/event*  virtio keyboard + mouse (dev:kbd0, dev:mouse0)
    /oath/ssh/         dropbear host keys (generated)
    dropbear           svc:sshd, keys from ssh:local
    seatd              svc:seatd (DRM seat)
    river              svc:river (glibc, pixman, libudev-zero, socket /run/user/0)
    sola-bus/call      svc:sola-bus / svc:sola-call (sockets /run/user/0)
    sola-river         svc:sola-river (bridge, not the compositor)
    sola-shell         svc:sola-shell (iced menubar; software GL; McMojave)
    /sbin/init -> ../usr/lib/oath/init
```

PID 1: mount proc/sys/dev/pts, tmpfs `/tmp` `/dev/shm` `/run`, cgroup2;
hostname from `host:local`; **converge** `net:net0`, `dev:*`,
`ssh:local`; then `svc:*`. Socket `/oath/run/init.sock`. Seeded
services: `svc:serial`, `svc:hold`, `svc:sshd`, `svc:seatd`, `svc:river`,
`svc:sola-bus`, `svc:sola-call`, `svc:sola-river`, `svc:sola-shell`.

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

Workspace crates: `oath-core`, `oath`, `oath-init`, `oath-make` (host
build CLI: `cargo make`). Artifacts in `build/` (gitignored).
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
[specs/2026-08-31-sola-dev.md](specs/2026-08-31-sola-dev.md)
