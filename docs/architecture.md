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

## As-built (2026-08-29)

QEMU x86_64 appliance. Serial is the console.

```
QEMU -kernel bzImage -initrd initrd.gz -drive virtio qcow2
  kernel (borrowed Linux 6.12) + initramfs
    /init = oath-init
    loads virtio_blk + btrfs modules
    mounts /dev/vda subvol=@ , chroot
    mounts subvolid=0 at /oath/run/fs
  disk (btrfs)
    @            live root
    @gen-N       readonly sibling generations
    /usr/lib/oath/init     PID 1 after pivot
    /usr/lib/oath/serial-login
    /bin/*                 symlink farm into /oath/store/pkg/<name>/bin/
    /oath/                 catalog
    /oath/store/pkg/busybox|btrfs|oath|hello/
    /sbin/init -> ../usr/lib/oath/init
```

PID 1: mount proc/sys/dev, hostname from `host:local`, **converge**
`svc:*` (start enabled, SIGTERM disabled), reap, listen
`/oath/run/init.sock`. Seeded services: `svc:serial`, `svc:hold`.

`oath apply` snapshots live `@` to sibling `@gen-N` under `/oath/run/fs`
(btrfs top-level). Undo restores catalog documents (including `store/`)
from that generation, not `/oath/run`. Fallback: copy the catalog tree
when the top-level is not mounted.

Telemetry: guest lines `oath-tel {json}` on stderr and `/oath/log/*.jsonl`.
`oath apply` on `pkg:*` creates or removes `/bin` symlinks into
`/oath/store/pkg/<name>/bin/`. Undo restores `store/` with the catalog
then converges links.

Host runs live under `build/runs/<id>/` (`cargo make run` / `probe`).

Workspace crates: `oath-core`, `oath`, `oath-init`, `oath-make` (host
build CLI: `cargo make` pack/run/probe). Artifacts in `build/` (gitignored).

**Target:**
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md) ·
[specs/2026-08-28-packages.md](specs/2026-08-28-packages.md) ·
[specs/2026-08-29-pkg-base.md](specs/2026-08-29-pkg-base.md)
