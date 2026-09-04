# Filesystem snapshots, own init, musl + glibc runtime

**Status:** idea (parked 2026-08-27). Not a freeze. Captures the second
founding pass: own PID 1, snapshot rollback, musl base with a glibc
*runtime* for foreign payloads. **Related:**
[open-questions.md](../open-questions.md) D2, D3, D4;
[2026-08-27-founding-brainstorm.md](2026-08-27-founding-brainstorm.md).

Do not implement from this file.

---

## Own init (locked)

Time is not a factor. There is no “wrap dinit to boot sooner.” A foreign
init would come with its own config dialect, and that dialect would
become the real OS.

**Right** means:

- PID 1 and the supervisor are Oath-owned (Rust).
- Services are catalog objects. Init has **no second config format**.
- Desired graph is in the catalog; actual (running, failed, restart
  count, logs) is queryable the same way.
- PID 1 duties are real: reap, signals, halt/reboot, early mounts,
  never exit.
- Supervision is small and fully describable on an INDEX page: restart
  policy, dependencies, readiness, cgroups v2.

Not systemd. Not a compatibility layer. Not a stepping-stone we intend
to delete after it has already defined how services are written down.

---

## Rollback is a filesystem snapshot

Three “undos” that distros usually keep separate:

1. Base OS update (A/B slots, OSTree).
2. Package transaction.
3. Config mutate (`oath apply`).

If the product filesystem can snapshot, those can be **one primitive**:
a generation. `oath apply` snapshots, mutates, records the generation
on the apply log. `oath undo` rolls desired *and* actual together.
Boot can pick a generation (bootloader + subvolume), which is the
image-update story without a second mechanism.

That is simpler to teach an agent than “A/B *and* a journal *and*
package rollback.”

**First candidate:** btrfs subvolumes. Known, in-tree, QEMU-friendly,
send/receive if we ever need to move generations. **T33** is that
move: one NFS copy, overwrite
([../specs/2026-09-03-backup-nfs.md](../specs/2026-09-03-backup-nfs.md)).
bcachefs is younger.
ZFS is excellent and a license/politics problem. Overlayfs+OSTREE is a
different ontology (we would be inheriting OSTree).

Not locked to btrfs yet. Locked-as-principle: **do not invent a parallel
undo that is blind to the filesystem.** QEMU qcow2 snapshots are a
host-side debug trick, not the product.

Catalog desired-state still exists (typed documents, diff, apply). The
snapshot is how the *world* rewinds so desired and actual do not drift.

---

## musl base, glibc as a runtime object

musl is the right *identity* for Oath-owned code: init, `oath`, catalog,
small userland. glibc as the base is how we become a worse Ubuntu.

Some payloads only exist as glibc binaries (large prebuilt stacks —
browsers are the usual example). That does not force glibc PID 1.

Two libcs **in one process** is not a hybrid. It is a broken loader.
Never `dlopen` a glibc library from a musl process.

| Layer | Libc |
|-------|------|
| Oath base (PID 1, `oath`, catalog, Oath-owned programs) | musl |
| A shipped payload that only exists as glibc | a **glibc runtime** object |

The glibc world is a catalog object (a sysroot / runtime with
`ld-linux-x86-64.so.2` and friends), not the OS identity. `oath` execs
the payload *into* that runtime.

**Later, optional:** rebuild those payloads against musl. Not required
to keep musl as the base, and not Phase 1.

**Not a year-one ABI promise** for random Linux binaries. The runtime
exists because we ship a payload that needs it, not because someone
copies an Ubuntu binary onto the box.
