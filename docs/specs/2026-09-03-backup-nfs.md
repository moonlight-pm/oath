**Date:** 2026-09-03
**Status:** target (freeze)
**Implementation:** partial
**Dogfood:** canto 2026-09-04 `svc:backup` last send generation 16 to
  `10.0.0.12:/mnt/alpha/backup/canto` (`canto.send` 2056610447 bytes,
  checksum match). Helper run by hand this boot (ESP initrd still old
  PID 1). NFS modules insmod’d live.
**Gaps:**
- NFS modules + helper packed for the **next** `cargo make build`;
  canto ESP initrd not rebuilt (reboot drops nfs.ko until then)
- `svc:backup` enable+apply oneshot needs the new PID 1
- no restore-in-installer
- QEMU image not rebuilt with this yet
- local `@gen-N` reaping still out
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Off-box backup (one NFS copy)

Extends D4 / [2026-08-27-catalog-and-oath-surface.md](2026-08-27-catalog-and-oath-surface.md)
(`snap`, generations),
[../manual/generations.md](../manual/generations.md) (as-built layout),
[2026-08-30-pkg-hosting.md](2026-08-30-pkg-hosting.md) (T20 peer as a
later destination). Does not replace local undo.

v0 is **one full copy** of the live system on an NFS volume,
**overwritten** each time we back up. No hourly history, no extra
copies of a ~4.2G tree. Local `@gen-N` stays the apply/undo primitive
(CoW on the system disk, not a second full image).

---

## Analysis

### The job

Canto’s reconstructable package set lives in this repo (`seed` +
`pack`). Live `/home`, catalog drift, and “which bytes were actually
on the box” do not. A wipe-and-reinstall from `cargo make install`
gets today’s pack, not yesterday’s machine. Off-box backup is that
inventory: the bytes.

Time Machine is the shape people mean: a volume somewhere else, a
consistent point in time, a way back after the disk dies. Apple’s
network target is a **disk image** on the share (sparse bundle)
because the share itself is not a snapshotting filesystem.

### What we already have

`oath apply` snapshots live `@` to a read-only sibling `@gen-N` at
`/oath/run/fs`. `snap:N` records `generation`, `parent`, `time`,
`reason`. That snapshot is the **whole** `@` (catalog, store, `/home`,
root). `oath undo` restores **catalog only** (`/oath` + store + `/bin`
converge). It is not a Time Machine restore of files.

The founding snapshot note already named the move format: **btrfs
send/receive**.

Local generations and off-box backup are two jobs on **one**
primitive:

| | Local `@gen-N` (today) | Off-box (this freeze) |
|--|------------------------|------------------------|
| When | `oath apply` | when we back up (explicit) |
| Where | same disk | NFS volume |
| Survive disk death | no | yes |
| How many | CoW siblings (apply history) | **one** full copy, overwrite |
| Undo / restore | `oath undo` (catalog) | receive the send stream onto btrfs |

Do not invent a second snapshotter (restic as identity, rsync
`--link-dest` trees, a `backup` kind). Do not snapshot the NFS mount
— NFS cannot `btrfs subvolume snapshot`.

### Why send, not rsync

A packed `@` is ~4.2G (CEF, Sola, glibc, store). rsync of the live
root during a backup races the filesystem and pays for the whole
tree every time. `btrfs send` of a **read-only generation** is a
consistent point; the stream *is* the generation.

v0 still sends a **full** stream and overwrites the previous file,
because we are not keeping a parent on the NAS. Incremental send
needs the parent present at the destination. That is a later
improvement, when the destination may hold more than one tree.

### Why one copy

Retention (hourly / daily / weekly) is Time Machine’s headline and
wrong for this disk: N copies of 4.2G. v0 keeps **one** off-box
image. The previous file is replaced only after the new send
finishes (temp + rename) so a failed backup does not destroy the
only copy.

Local `@gen-N` is not “multiples of 4.2G.” It is CoW on the system
disk. Apply/undo keeps using it. Reaping local gens is a separate
gap.

### Why NFS, not “another Oath host” first

T20 already says another Oath store is a valid **package** origin.
A peer with real btrfs could `btrfs receive` in place. Canto’s
practical target today is a LAN NAS. A dumb NFS export that holds
**one file** is that. The send stream is the same object we would
later receive on a peer or into an installer.

### Why not a new kind or verb

The generation is already `snap:N`. The process is already `svc`.
The helper lives at `/lib/oath` like `run-compositor`. `oath apply`
on that svc is “Back Up Now.” No `oath backup`, no `kind:backup`,
no fstab dialect.

---

## Locks this freeze owns

- **Source is a read-only generation of whole `@`.** Send never
  walks live `@` while it mutates. If the latest `@gen-N` is missing
  or too old for this run, take a snapshot first (`reason` includes
  backup). That local CoW is cheap; it is not a second NFS copy.
- **Destination is NFS.** One stream file on the export, named
  stably (e.g. `canto.send` for host `canto`). Not a sparse image in
  v0. Not a directory of dated trees.
- **One copy. Overwrite.** No hourly, daily, or weekly retention on
  NFS. No parent chain stored off-box.
- **Atomic replace.** Write `*.send.tmp` (+ sidecar tmp), fsync,
  rename over the live `*.send`. On failure the previous copy stays.
- **Sidecar** next to the stream (`canto.json` or equivalent):
  hostname, generation, time, size, content checksum of the stream.
  The NAS is not the catalog; the sidecar is how a later restore
  knows what the blob is. Apply verifies checksum on restore when
  we have restore; v0 at least writes it.
- **Explicit trigger.** No clock. `svc:backup` (`restart: never`,
  `wants` network), `exec` is `/lib/oath/backup-send` plus the NFS
  target (`host:/export` or a mount path). Enable + `oath apply` runs
  it. It mounts if needed, sends, unmounts or leaves the mount as
  the helper decides, exits. PID 1 does not loop it.
- **No new kind. No new verb.** Destination lives in `svc:backup`
  desired `exec` (same pattern as dropbear flags). Record last
  successful send on that svc’s **actual** (generation, time, dest,
  checksum) so `oath get svc:backup` is the examination surface.
- **Undo is not restore.** `oath undo` stays catalog rewind on the
  live box. Getting files or a dead disk back is `btrfs receive` of
  the stream onto a btrfs (installer or scratch subvol). Do not exec
  from the backup file.
- **Scope is whole `@`.** Not home-only. Store extents are part of
  “what was on canto.”

---

## Courage test (when implemented)

1. NFS client can mount the export. `svc:backup` desired names it.
2. `oath apply` on `svc:backup` creates or uses a read-only
   `@gen-N`, writes a full send stream, replaces the previous file
   only on success. Sidecar matches the stream checksum.
3. A second apply overwrites. One `*.send` (and sidecar) on the
   export, not two.
4. Kill the send mid-way: the previous `*.send` is still intact.
5. `oath get svc:backup` shows last generation / time / checksum.
6. `oath undo` still only restores catalog on the live box.
7. `btrfs receive` of that stream onto a scratch btrfs yields a
   subvolume that contains `/oath` and `/home` from that generation.

---

## Later (not this freeze)

Ordered so each step keeps the same stream identity.

1. **NFS in the image** — kernel module + mount helper; this is the
   first implementation slice.
2. **Restore documented in the installer** — name the stream, verify
   checksum, receive onto `@` instead of packing from the build host.
   That is the wipe-canto path that keeps `/home`.
3. **Checksum pin on restore** — refuse a stream that does not match
   the sidecar (T32-shaped: verify, do not choose).
4. **Incremental send** — keep the parent generation on the
   destination (second file, or a real btrfs receive). Only when the
   NAS may hold more than one tree. Cuts the 4.2G rewrite.
5. **Retention** — hourly/daily/weekly once dest space is a decision,
   not a default. Policy is catalog (`snap` records + a keep count),
   not a second tool.
6. **Clock** — `restart: always` with a sleep, or a timer, on
   `svc:backup`. Still the same helper. v0 stays explicit apply.
7. **Sparse btrfs image on NFS** — Time Machine sparse-bundle
   analogue: loop file, `btrfs receive`, browse with
   `mount -o subvol=@gen-N`. Same send, nicer look. More failure
   modes (loop, nfs cache, image growth).
8. **Peer Oath receive (T20)** — another box with real btrfs is a
   better dest than a dumb NAS when we have one. Stream format
   unchanged.
9. **File browse** — Sola or `oath get` pointing at a received
   read-only gen. Exec still goes through `/bin` on a *live* root,
   not the backup.
10. **Local gen reaping** — independent of NFS; system disk fill.
    Do not conflate with “one off-box copy.”

---

## Out (v0)

- Hourly / multiple NFS copies / Time Machine retention
- rsync/restic/borg as the product identity
- Snapshotting NFS, or treating NFS as btrfs
- `kind:backup`, `kind:mount`, `oath backup`
- Home-only send
- Clock-triggered send
- Sparsebundle / loop image
- Installer restore (later)
- Reaping `@gen-N` on the system disk
- Encrypting the stream (later; NAS trust is LAN for v0)
- New `host:local` field (dest is `svc` exec)

## Amends

- **D4 / `snap`:** off-box is send of a generation, not a parallel
  undo. `oath undo` does not grow a restore-from-NFS meaning.
- **T20:** peer receive is a later dest; NFS file is v0.
- **T32:** sidecar checksum is the same pin idea for a stream, not
  a pack hash in the store path.
