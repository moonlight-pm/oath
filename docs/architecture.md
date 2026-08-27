# Architecture

**Role:** as-built system map (what the code and runtime look like **now**).
**Not** the place for multi-feature roadmaps or session priority.

| Concern | Document |
|---------|----------|
| Capability maturity | [capabilities.md](capabilities.md) |
| Target design freezes | [specs/](specs/) |
| Session priority + dogfood | Root [CURRENT.md](../CURRENT.md) |
| How docs fit together | [progress-model.md](progress-model.md) |
| Product docs | [manual/](manual/) — **shipped only** |

---

## As-built (2026-08-27)

QEMU x86_64 appliance. Serial is the console.

```
QEMU -kernel bzImage -initrd initrd.gz -drive virtio qcow2
  kernel (borrowed Linux 6.12) + initramfs
    /init = oath-init
    loads virtio_blk + btrfs modules
    mounts /dev/vda subvol=@ , chroot
  disk (btrfs, subvol @)
    /usr/lib/oath/init     PID 1 after pivot
    /usr/lib/oath/serial-login
    /bin/oath
    /bin/busybox (applets, including sh)
    /bin/btrfs
    /oath/                 catalog
    /sbin/init -> ../usr/lib/oath/init
```

PID 1: mount proc/sys/dev, hostname from `host:local` desired, spawn
`svc:*`, reap, listen `/oath/run/init.sock`.

`oath apply` snapshots (btrfs subvolume of `/` into `/.oath-gens/N` when
`btrfs` is present; otherwise copies the catalog tree), then converges.

Telemetry: guest lines `oath-tel {json}` on stderr and `/oath/log/*.jsonl`.
Host runs live under `build/runs/<id>/` (`image/run.sh`, `image/probe.py`).

Workspace crates: `oath-core`, `oath`, `oath-init`. Image scripts:
`image/build.sh`, `image/run.sh`. Artifacts in `build/` (gitignored).

**Target:**
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md)
