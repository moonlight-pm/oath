# Open questions

Unresolved **design forks**. Not the implementation backlog
(that lives in roadmap / capabilities / plans).

Agents: if work depends on a Decision point, **stop and ask**. Record the
answer in the decision log and update CURRENT locks.

Founding D1–D9 and the catalog freeze’s technical locks are **closed**.

---

## Decision points (ask human)

None open. T22 libinput without udevd is closed (libudev-zero;
path fallback in wlroots). T21 River-first is closed
(River/wlroots forks in). T23 session stack is closed (`pkg:sola`
+ session `svc:*`; no nested PM). T24 development layout is closed
(one `pkg:sola` blob; develop Sola on Nix until Oath is the host).
T25 session manager is closed (`svc:sola-session` in that blob).

---

## Open technical questions

- Bootloader hook to boot `@gen-N` (layout is `@` + `@gen-N` at
  `/oath/run/fs`; picker not built).
- PID 1 notify socket bytes (`/oath/run/…`) — implementation detail;
  must not become a second config format.
- How much of coreutils we replace vs busybox vs our own (Phase 1 may
  borrow busybox).
- glibc runtime object layout — v0 locked in T21 as sealed
  `pkg:glibc` (loader + libs in the store). Not a second libc in
  PID 1.

---

## Closed

### D1 — First dogfood shape (P0) — locked 2026-08-27

QEMU headless appliance. No desktop. No bare-metal installer in Phase 1.
Success is a VM whose catalog an agent can read. **x86_64**, **serial
console**.

### D2 — libc and foreign ABI (P0) — locked 2026-08-27

**musl base.** glibc may exist later as a catalog runtime object. Never
mix libcs in one process.

### D3 — Init / supervisor (P0) — locked 2026-08-27

Own PID 1 + supervisor in Rust. `svc` objects are the only service
config.

### D4 — Package and update model (P0) — locked 2026-08-27

Packages as catalog objects (later). Rollback via **btrfs** subvolume
generations. qcow2 snapshots are host debug only.

### D5 — Filesystem layout (P0) — locked 2026-08-27

Catalog tree is **`/oath`**. Agents do not edit `/etc`. Compatibility
shard only to execute what we shipped.

### D6 — Desired-state store (P1) — locked 2026-08-27

Pretty-printed JSON under `/oath/objects/<kind>/<name>/{desired,actual,meta}.json`.
Apply log JSONL. Undo is the filesystem snapshot.

### D7 — Agent coupling (P1) — locked 2026-08-27

`oath` text + `--json`. MCP later. No vendor API in the schema.

### D8 — Bootstrap / build host (P1) — locked 2026-08-27

Build tools are not the runtime. Borrowed prebuilts ok in Phase 1.

### D9 — License (P2) — locked 2026-08-27

MIT. Copyright (c) Joshua Kifer. [`LICENSE`](../LICENSE).

### T1 — Catalog path — locked 2026-08-27

`/oath`. INDEX at `/oath/INDEX.md`.

### T2 — Object identity — locked 2026-08-27

`kind:name`. No UUIDs in v0.

### T3 — Schema language — locked 2026-08-27

JSON Schema 2020-12 until it hurts. Kind prose in Markdown.

### T4 — `oath` views — locked 2026-08-27

One verb set. Text default, `--json` same facts. MCP later.

### T5 — v0 kinds — locked 2026-08-27

`host`, `svc`, `snap`. No net/pkg/dev in Phase 1. Serial is how the
agent reaches the VM.

### T6 — Snapshot filesystem — locked 2026-08-27

btrfs for Phase 1.

### T7 — Agent reachability — locked 2026-08-27

QEMU serial. SSH later.

### T8 — Architecture — locked 2026-08-27

`x86_64` only in Phase 1.

### T9 — Principal — locked 2026-08-27

Root on serial is the owner. Agent is not a second Unix user. Log
uid + tty.

### T10 — Confirm class — locked 2026-08-27

`mutate` vs `confirm`. Halt, wipe, boot-generation (except undo last)
need `--confirm`. Agents do not pass it unless the owner asked.

### T11 — Package store — locked 2026-08-28

`/oath/store/pkg/<name>/` is the package tree. `/bin` is a symlink farm
to that tree. No hard links. No second PATH. No unpack into `/usr`.

### T12 — Package surface — locked 2026-08-28

Kind `pkg`. Same `oath` verbs (`set` / `apply` / `undo`). No `oath
install`, no apt dialect. v0 field `present`. No fetch, no glibc, no
from-source in this slice.

### T13 — Phase 3 first package — locked 2026-08-28

Canary `pkg:hello` only. Do not wrap busybox / btrfs / `oath`. `svc`
`wants` stays a listed gap.

### T14 — Base packages, not removable — locked 2026-08-29

`pkg:busybox`, `pkg:btrfs`, `pkg:oath` are present at seed. Applets
are one busybox package. `actual.removable=false`: `present=false` is
refused (not confirm). hello stays removable. `wants` still out.

### T15 — Network canary — locked 2026-08-29

Kind `net`, object `net:net0`. QEMU user + virtio-net. Static
`10.0.2.15/24` via `10.0.2.2`. NIC renamed to `net0`. Serial stays
reachability. No SSH, DHCP, `dev`, or pkg fetch in this slice.

### T16 — SSH keys and DHCP — locked 2026-08-30

No baked login private key. Host keys generated under `/oath/ssh/`.
Owner pubkeys in `ssh:local`. Dropbear `svc:sshd`, root only.
`net:net0` `ipv4` may be `dhcp`. QEMU hostfwd 2222; optional
`OATH_BRIDGE`. Network install injects **public** keys.

### T17 — Device inventory — locked 2026-08-30

Kind `dev`. v0 `dev:vda`, `dev:net0`, `dev:ttyS0`. Not removable. Probe
sysfs; do not load modules from the catalog. No udev. Unix floor:
tmpfs `/tmp` `/dev/shm` `/run`, cgroup2. `/proc` was already mounted.

### T18 — wants + fetch — locked 2026-08-30

`svc.wants` is start order; cycles refuse apply. `pkg.url` wget into
the store. No package repository. No new verbs.

### T19 — Display canary — locked 2026-08-30

virtio-gpu, `/dev/dri/card0`, `dev:card0`. gtk window when DISPLAY is
set. Probe headless. No Wayland, River, or Sola in this slice.

### T20 — Package hosting — locked 2026-08-30

`pkg.url` is the origin. Another Oath host that has a store tree may
serve those bytes; the peer sets `url` and applies. No canonical
archive, no `repo` kind, no git-as-store. Apply does not clone.
Serving the store, hashes, and discovery are deferred.

### T21 — Sola on Oath, River first — locked 2026-08-30

Patched River as `pkg:river` + `svc:river` (wants `svc:seatd`).
PID 1 is the only supervisor. glibc is sealed `pkg:glibc`.
`forks/river` and `forks/wlroots` are submodules. First-party pkg
sources under `apps/`. Full Sola session and udev/libinput out of
this slice. `oath-sola` is T23.

### T22 — Libinput without udev — locked 2026-08-30

libinput stays inside `pkg:river`. libudev is libudev-zero
(borrowed). wlroots falls back to the path backend on
`/dev/input/event*` if udev finds nothing. evdev module.
`dev:kbd0` / `dev:mouse0`. No udevd. No `pkg:libinput`. No new kind.

### T23 — Sola session on Oath — locked 2026-08-30

`moonlight-pm/oath-sola` is the Oath-compat fork (private
submodule `forks/sola`). PID 1 is the only supervisor. Bits are
`pkg:sola`; processes are `svc:sola-bus`, `svc:sola-call`,
`svc:sola-river` (bridge), `svc:sola-shell`. Do not pack
`crates/sola`. Daily-driver Sola stays NixOS. Not a new kind.
Sola-generic fixes cherry-pick to Sola main, then pull; Oath-compat
stays on the fork ([forks/README.md](../forks/README.md)).

### T25 — Sola session manager — locked 2026-08-31

`svc:sola-session` is the LaunchApp / CloseApp owner, fifth ELF in
`pkg:sola`. Direct spawn (no systemd). Dual-mode logs and Wayland.
Not a nested process manager. Kit apps still out.

### T24 — Sola / app development layout — locked 2026-08-31

Development versions are package generations of the real objects
(apply / undo). No second PATH, no `/opt/sola` on Oath, no
`pkg:sola-dev`, no nested PM. Keep one `pkg:sola` blob. Develop
Sola on Nix until Oath is the host; re-address then. Dual-mode
seat in `oath-sola`.

---

## Decision log

| Date | ID | Decision | Where recorded |
|------|-----|----------|----------------|
| 2026-08-27 | D1–D9 | Founding locks | this file Closed; [CURRENT.md](../CURRENT.md) |
| 2026-08-27 | T1–T10 | Catalog freeze technical locks | this file; [specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md) |
| 2026-08-28 | T11–T13 | Package store + canary | this file; [specs/2026-08-28-packages.md](specs/2026-08-28-packages.md) |
| 2026-08-29 | T14 | Base pkgs not removable | this file; [specs/2026-08-29-pkg-base.md](specs/2026-08-29-pkg-base.md) |
| 2026-08-29 | T15 | Net canary static net0 | this file; [specs/2026-08-29-net.md](specs/2026-08-29-net.md) |
| 2026-08-30 | T16 | SSH catalog keys + DHCP | this file; [specs/2026-08-30-ssh-and-dhcp.md](specs/2026-08-30-ssh-and-dhcp.md) |
| 2026-08-30 | T17 | Device inventory + Unix floor | this file; [specs/2026-08-30-devices.md](specs/2026-08-30-devices.md) |
| 2026-08-30 | T18 | svc wants + pkg url fetch | this file; [specs/2026-08-30-wants-and-fetch.md](specs/2026-08-30-wants-and-fetch.md) |
| 2026-08-30 | T19 | virtio-gpu display canary | this file; [specs/2026-08-30-display.md](specs/2026-08-30-display.md) |
| 2026-08-30 | T20 | pkg.url origin; Oath hosts as store | this file; [specs/2026-08-30-pkg-hosting.md](specs/2026-08-30-pkg-hosting.md) |
| 2026-08-30 | T21 | Sola on Oath; River first; forks/ | this file; [specs/2026-08-30-sola.md](specs/2026-08-30-sola.md) |
| 2026-08-30 | T22 | libinput via libudev-zero; no udevd | this file; [specs/2026-08-30-libinput.md](specs/2026-08-30-libinput.md) |
| 2026-08-30 | T23 | oath-sola fork; session as `svc:*`; no nested PM | this file; [specs/2026-08-30-oath-sola.md](specs/2026-08-30-oath-sola.md) |
| 2026-08-31 | T24 | Sola/app dev: apply/undo; one sola blob; Nix host until Oath is | this file; [specs/2026-08-31-sola-dev.md](specs/2026-08-31-sola-dev.md) |
| 2026-08-31 | T25 | sola-session as svc; LaunchApp owner; still one sola blob | this file; [specs/2026-08-31-sola-session.md](specs/2026-08-31-sola-session.md) |
