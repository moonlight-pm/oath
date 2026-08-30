# Live re-init, and changing a box from another machine

**Status:** idea (parked 2026-08-29). Not a freeze. Do not implement
from this file.
**Related:** catalog freeze apply/undo; package freeze (no fetch);
roadmap Phase 4 network, Phase 6 install/updates.

---

## Live apply; reboot is not the installer

Userspace change should take effect when `oath apply` converges — no
reboot required. That is already the catalog model (hostname, `svc`,
and the `pkg:hello` canary). Reboot in a probe is **persistence
proof**, not how you install.

Later, a stronger form: instead of rebooting the kernel, tear down
userspace and re-run PID 1 converge (or kexec only when the kernel
actually changed). Kernel, modules, and bootloader still need a real
reboot or kexec. Do not invent a second “reload” verb; it is still
`apply`.

Not Phase 3.

---

## Change the box from another machine

Install the OS, upgrade the base, or apply packages from a second
computer on the network. Same objects and verbs; the network is how
the agent **reaches** the catalog, not a parallel package UI.

Serial is today’s reachability. SSH / a push of a generation belong
with Phase 4 (net) and Phase 6 (install and updates), not the first
package canary (no fetch).

---

## Not this: a VCS as the live OS

Git, Jujutsu, Pijul, etc. are fine for **this repo**. They are not the
running system. Live history is already btrfs generations plus the
apply log. A checkout under `/oath` would be a second ontology.
Locked as T20: [../specs/2026-08-30-pkg-hosting.md](../specs/2026-08-30-pkg-hosting.md).
