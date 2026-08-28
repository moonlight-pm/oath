# Plan — Phase 1 QEMU skeleton

**Date:** 2026-08-27
**Status:** complete (2026-08-28)
**Freeze:** [../specs/2026-08-27-catalog-and-oath-surface.md](../specs/2026-08-27-catalog-and-oath-surface.md)
**Proof:** courage test in that freeze (hostname survives reboot; undo works)

At most one open plan. This is it. Checkboxes are implementation work.
CURRENT **Now** has moved to Phase 2. This plan is kept as history.

Phase 1 is an **x86_64 QEMU** qcow with **btrfs** root, **serial** as
the console, musl userland, our PID 1, `/oath` populated, `oath` binary
on PATH.

Borrowed prebuilts (kernel build, busybox) are allowed. They are not
the identity. Replace inward later.

---

## 1. Image and boot

- [x] Kernel: borrowed vanilla Linux bzImage + modules (virtio, btrfs,
      serial). Not a custom config yet.
- [x] qcow2 with one btrfs filesystem; live root is subvolume `@`.
- [x] QEMU via `oath-make run` (x86_64, serial, virtio-blk).
- [x] PID 1 (`oath-init`): mounts, hostname from `host:local`, `svc:*`,
      reap, halt/reboot syscalls via `oath apply`.
- [x] `svc:serial` root shell on serial (`serial-login`).

## 2. Catalog in the image

- [x] `/oath` tree as in the freeze, present at first boot.
- [x] JSON Schema + Markdown for `host`, `svc`, `snap`.
- [x] Objects: `host:local`, `svc:serial`, `snap:current` (generation 0).
      `snap:N` created on apply.
- [x] `INDEX.md` generated.
- [x] `oath` binary: `ls`, `schema`, `get`, `set`, `diff`, `apply`,
      `undo`, `log`, `--json`, `--confirm`, exit 3 on confirm-class.

## 3. Converge handlers

- [x] `host` hostname: sethostname + persist; boot path reapplies.
- [x] `host` power reboot/halt: `confirm` (refuses without `--confirm`).
- [x] `svc`: PID 1 is the handler; socket notify exists.
- [x] `apply` snapshots first (`btrfs subvolume snapshot` when `btrfs`
      is in the image); apply log written.
- [x] `undo` restores last apply (hostname live-tested).

## 4. Courage test (must all pass)

- [x] Fresh VM: `oath` / INDEX readable.
- [x] `oath ls` lists `host:local`.
- [x] Change hostname, `apply`, **reboot**, actual matches (`oath-make probe` boot2).
- [x] `oath undo` restores hostname on the live VM.
- [x] `oath apply` of `power=reboot` without `--confirm` exits 3.
- [x] Serial still works after reboot (probe boot2).

## 5. Docs in the same change as code

- [x] `docs/capabilities.md` rows: boot, catalog, objects, oath-cli,
      snap, svc — status/gaps/dogfood.
- [x] `CURRENT.md` dogfood: how to launch the QEMU wrapper.
- [x] `docs/architecture.md` as-built: processes, paths, image.
- [x] Freeze header: Implementation / Dogfood / Gaps.
- [x] Manual: [../manual/qemu.md](../manual/qemu.md) limited.

---

## Explicitly not this plan

Packages, devices, real network objects, glibc runtime, SSH, installer,
MCP, aarch64, replacing busybox.
