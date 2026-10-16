# Capabilities

What exists today and what is missing, per capability. Target design is
in [specs/](specs/); priority is in [CURRENT.md](../CURRENT.md). Keep
this file under 800 words. Update a row in the same change as the code.

**Status:** `shipped` works and is tested · `partial` works with listed
gaps · `target` designed, not built.

**As of:** 2026-10-16

## Core

| Capability | Status | Works | Main gaps |
|---|---|---|---|
| Catalog | partial | `kind:name` objects with desired/actual/meta; JSON Schema per kind; generated `INDEX.md`; locked atomic writes; typed `set`; id grammar; `oath new` / `oath rm` (svc, pkg) | Schemas are hand-written, not generated from the types; `new` fills no defaults |
| `oath` CLI | shipped | `ls get set new rm diff apply keep undo log logs schema fmt build`, `--json`; bare `oath` prints the INDEX; apply returns after PID 1 converged | MCP adapter is roadmap phase 7 |
| Generations | partial | checks before the snapshot (`refused` changes nothing); every outcome logged; undo is a stack restoring only the touched objects' spec; lockout-class changes go on probation and PID 1 undoes them unless `oath keep` runs from a new session | A catalog reboot takes a generation |
| Services (PID 1) | partial | runs only applied specs; per-service logs (`oath logs`); exit codes, restart count, backoff, `failed`; a cgroup per service; orderly shutdown on `reboot`/`poweroff`; a wants cycle skips only its members | Shutdown is not in reverse `wants` order |
| Packages | partial | hash-in-path store, `/bin` farm, pins, `needs` graph, `components.json` + advisories, fetch by `url`; publish: licensed profile packages only, each with its plan and source pack | Store integrity not verified at boot |
| Plans (builds) | shipped | `plan` kind, `oath fmt`, `oath build`; netless sandbox in its own PID namespace, nothing from the build host; cache keyed on plan and inputs; atomic store writes; logs in `/oath/log/plan/`; every image package, kernel and Oath binaries included, is a plan product | — |
| Toolchain and native packages | partial | `cargo make bootstrap` makes the first Clang/LLVM 23.1.3 + musl 1.2.6; `cargo make pkgs`: 27 plan products from `pkgs/` (sha256-pinned sources, upstream OpenPGP and Sigstore signatures checked, SPDX licences, `pkgs/lock.json`), among them `pkg:llvm`, the kernel and the vendor's Rust 1.99.1, which builds `pkg:oath` against `pkg:llvm`'s musl; rebuilds are bit-identical | Unsigned upstreams (Perl among them): sha256 only |
| Network | partial | `net:net0` dhcp (default) or static; NIC renamed to `net0`; a supervised DHCP client renews the lease; time from pool.ntp.org (`svc:time`) | `actual.lease` shows the first lease; one NIC, chosen at boot |
| SSH | shipped | dropbear server as `home`, keys from `ssh:local`, root denied; OpenSSH client tools and `sftp-server` (no OpenSSL, ed25519); `sudo` for `home` (clean env, groups dropped) | — |
| Devices | partial | modules load by modalias at boot and on kernel request; `dev:net0`, `dev:ttyS0` inventory | No hotplug; no discovery (disks are not listed) |
| Seed and overlays | shipped | profile `base`, free of test fixtures; `svc:serial` idles with no UART; profiles extend each other; `oath seed --overlay`; `install --overlay` | — |
| Kernel | shipped | one kernel, `pkg:linux`: a plan built by `pkg:llvm` (`LLVM=1`; 7.3-rc1, sha256-pinned, Oath fragment and patches; depmod index from `.modinfo`); pack refuses a vermagic mismatch; canto runs it | — |
| Boot | partial | initrd from our PID 1 + native busybox and firmware; switch_root into `@`; `oath-efi` is the only loader; ESP keeps current + last 5 boots; `panic=10`; boots canto through Apple firmware | `cargo make esp` runs only on the Oath machine; no remote update but reinstall |
| Image build | shipped | `cargo make build` packs `pkg:oath` and the locked packages into an immutable `build/out/<id>/` with a manifest of every shipped pack; install and `esp` take its id | — |
| Install | partial | installer ramdisk over SSH (any login shell); GPT + ESP + btrfs `@`; `--overlay`; makes and pins the host key; `--qemu` rehearsal; `--usb` stick; installs one checked build output; canto reinstalled unattended | — |
| Test harness | shipped | `cargo make check`: one QEMU per case, real reboots, known gaps per phase, `check-pass` stamp per output; cases make their own fixtures with `oath new`; `--target`: the `metal` cases on an installed machine at any generation, undone after each | — |

## Payload (outside core)

| Payload | Status | Notes |
|---|---|---|
| Desk (River, Sola) | target | Returns when River, wlroots, mesa, Sola and their deps build natively. |
| Wicket host | target | Left core (R1); Wicket will ship its own packages and overlay. buccina still runs the trial image. |
| Steam + Minecraft desk | target | Future requirement (R2): native desk, Steam runtime, JVM. |
