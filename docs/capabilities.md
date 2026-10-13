# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row in the same change as the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-13 (after the review cut; nothing below has been
booted on metal since the cut)

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | partial | `kind:name` objects with desired/actual/meta, JSON Schema per kind, generated `INDEX.md`, apply log | Writes are not atomic and take no lock; set and apply check key names only (a bad `svc` value applies and PID 1 then stops it); no id grammar; no `oath new` |
| `oath` CLI | partial | `ls get set diff apply undo log schema fmt build`, `--json` | No `new`/`rm`/`logs`; bare `oath` shows only the first 24 lines of INDEX |
| Generations | partial | apply snapshots `@` to `@gen-N`; `snap:current`; undo; survives a catalog reboot (checked) | Snapshot is taken before validation; a failed apply is not logged; undo restores `log/` (erasing the apply it reverts) and `store/`; a catalog reboot takes a generation |
| Services (PID 1) | partial | `svc` exec/wants/restart/enabled/user/env/bind_low_ports; TZ from `host:local.timezone` | A wants cycle starts nothing; an unparsable svc is stopped; no service logs or exit codes; no backoff or failed state; no cgroups; no shutdown sequence; `chroot` not `switch_root`; fallback shell never reaps |
| Packages | partial | hash-in-path store, `/bin` farm, pins, `needs` graph, description/home, `components.json` + advisories, fetch by `url` | Store integrity is assumed, not verified at boot; publish has no allowlist; RPATHs go through `live` |
| Plans (builds) | partial | `plan` kind, `oath fmt`, `oath build`, netless sandbox, pinned inputs | Cache key ignores the plan hash; no package in the image is built by a plan yet |
| Network | partial | `net:net0` dhcp (default) or static; NIC renamed to `net0` | DHCP runs inside apply, not as a service; one NIC only |
| SSH | shipped | dropbear as `home`, keys from `ssh:local`, root denied, sudo for `home` | `sudo` helper is minimal (no setgroups, keeps env) |
| Devices | partial | `dev:vda`, `dev:net0`, `dev:ttyS0` inventory | Names are hard-coded; modules load from a fixed list, not by modalias |
| Seed and overlays | shipped | profile `base` (`crates/oath-core/seed/`), profiles can extend each other; `oath seed --overlay`; `install --overlay` | PID 1 reseeds if `INDEX.md` is missing |
| Kernel | shipped | one kernel from `image/build-linux.sh` (7.3-rc1, sha256-pinned, Oath fragment and patches); pack refuses a release/vermagic mismatch | Built by hand before `cargo make build`; not yet a plan |
| Boot | partial | initrd from our PID 1 + busybox; `oath-efi`; ESP keeps current + last 5 boots; `cargo make esp` runs only on an Oath machine | No panic hook or `panic=` on metal; plain `reboot` does nothing (PID 1 ignores the signal; use `oath` power=reboot) |
| Image build | partial | `cargo make build [--profile base]`, QEMU `run/up/start/ssh` | Userspace tools come from `image/tools.nix` (Nix); output is not immutable or manifest-checked |
| Install | partial | installer ramdisk over SSH, GPT + ESP + btrfs `@`, `--overlay`, `--qemu` rehearsal, `--usb` stick; refuses an image `cargo make check` has not passed | Copies the mutable `build/stage`; not yet a manifest-checked output |
| Test harness | shipped | `cargo make check`: one QEMU per case from `check/cases.toml`, real reboots, known gaps marked per phase, `build/check-pass` stamp | 8 gap cases document the phase 3–4 defects |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (River, Sola) | target | Removed in the cut with its Nix packs. Returns when River, wlroots, mesa, Sola and their deps build natively. canto still runs the 2026-09 desk image. |
| Wicket host | target | Left core (R1). Wicket will ship its own packages and overlay once the package standard is settled. buccina still runs the Wicket trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): needs the native desk, Steam's runtime and a JVM. |
