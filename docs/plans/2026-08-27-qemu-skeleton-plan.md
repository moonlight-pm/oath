# Plan — Phase 1 QEMU skeleton

**Date:** 2026-08-27
**Status:** open (active)
**Freeze:** [../specs/2026-08-27-catalog-and-oath-surface.md](../specs/2026-08-27-catalog-and-oath-surface.md)
**Proof:** courage test in that freeze (hostname survives reboot; undo works)

At most one open plan. This is it. Checkboxes are implementation work.
CURRENT **Now** points here.

Phase 1 is an **x86_64 QEMU** qcow with **btrfs** root, **serial** as
the console, musl userland, our PID 1, `/oath` populated, `oath` binary
on PATH.

Borrowed prebuilts (kernel build, busybox) are allowed. They are not
the identity. Replace inward later.

---

## 1. Image and boot

- [ ] Kernel: vanilla Linux, our config, virtio, btrfs, serial console,
      no need for a desktop stack.
- [ ] qcow2 with one btrfs filesystem; live root is a subvolume.
- [ ] QEMU wrapper in-repo: `x86_64`, `-serial stdio` (or equivalent
      that an agent can attach to), virtio-blk, no GUI requirement.
- [ ] PID 1 is our binary: mount `proc`/`sys`/`dev`/root, set hostname
      from `host:local` desired, converge `svc:*`, reap, halt/reboot.
- [ ] `svc:serial` gets a root shell (or a login that is root) on the
      serial so the next command can be `oath`.

## 2. Catalog in the image

- [ ] `/oath` tree as in the freeze, present at first boot.
- [ ] JSON Schema + Markdown for `host`, `svc`, `snap`.
- [ ] Objects: `host:local`, `svc:serial`, `snap:current`, `snap:1`
      (or empty snap list with generation 0 documented).
- [ ] `INDEX.md` generated; honest about what is there.
- [ ] `oath` binary: `ls`, `schema`, `get`, `set`, `diff`, `apply`,
      `undo`, `log`, `--json`, `--confirm`, exit 3 on confirm-class.

## 3. Converge handlers

- [ ] `host` hostname: sethostname + persist in catalog; boot path
      reapplies.
- [ ] `host` power reboot/halt: `confirm`.
- [ ] `svc`: PID 1 is the handler; notify after apply.
- [ ] `apply` takes a btrfs snapshot first; log line written.
- [ ] `undo` restores the last apply’s snapshot (hostname included).

## 4. Courage test (must all pass)

- [ ] Fresh VM: `oath` (no args) and `cat /oath/INDEX.md` make sense
      to a model that has never seen Oath.
- [ ] `oath ls` lists `host:local`.
- [ ] Change hostname, `apply`, reboot, actual matches.
- [ ] `oath undo`, hostname (and catalog) match the parent generation.
- [ ] `oath apply` of `power=reboot` without `--confirm` exits 3.
- [ ] Serial still works after reboot.

## 5. Docs in the same change as code

- [ ] `docs/capabilities.md` rows: boot, catalog, objects, oath-cli,
      snap, svc — status/gaps/dogfood.
- [ ] `CURRENT.md` dogfood: how to launch the QEMU wrapper.
- [ ] `docs/architecture.md` as-built: processes, paths, image.
- [ ] Freeze header: Implementation / Dogfood / Gaps.
- [ ] Manual: only what actually works (INDEX-on-box is the operator
      doc; repo manual if we have a host-side wrapper).

---

## Explicitly not this plan

Packages, devices, real network objects, glibc runtime, SSH, installer,
MCP, aarch64, replacing busybox.
