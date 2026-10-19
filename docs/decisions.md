# Decisions

What has been decided, and what still needs Joshua. Agents ask before
acting on anything under **Open**. Replaces `open-questions.md`
(history in git, tag `t37-t39-final`).

## Open — ask the human

- **Steam + Minecraft desk.** Which machine, and when. Not before the
  native desk exists.
- **Permissive alternatives.** Joshua (2026-10-16) would like
  permissive replacements for the GPL packages (btrfs-progs,
  util-linux, dosfstools) some day; not now. R17 and R18 (GCC, glibc,
  the GNU userland) moved the toolchain, libc and tools the other way.

## 2026-10-13 review (briefing D1–D8)

| ID | Decision | Status |
|----|----------|--------|
| R1 | Wicket leaves the core. Products wait for Oath to mature, then bundle themselves with the package standard. | Done in the cut |
| R2 | Delete T37 (Steam, gamescope, sola-arcade) and T39 (Omarchy/Hyprland). Tag `t37-t39-final`. A Steam + Minecraft desk is a future requirement. | Done in the cut |
| R3 | The seed is data: profiles `base` and `dev`; machine facts are install overlays. | Done in the cut |
| R4 | Every catalog writer takes an exclusive file lock (`flock`) and writes temp-file-then-rename; PID 1 reads only `applied`. | Decided 2026-10-13; phase 3 |
| R5 | Delete the probe; add a headless QEMU harness that gates installs. | Probe deleted; harness next |
| R6 | One kernel: own-built, sha256-pinned, vermagic-checked. No Nix kernel. | Done in the cut |
| R7 | Native packages from the outset: built from source by an Oath plan, or the vendor's release pinned by sha256. Never another distro's packages. Build-host compilers bootstrap only. | Done 2026-10-14 (phase 5); the toolchain and the kernel are plan products since 2026-10-15 (`pkg:gcc` and `pkg:glibc` since 2026-10-17, R17), and the Oath binaries (built by the vendor's Rust, `pkg:rust`) since 2026-10-16; only stage 0 uses the host's gcc, and only `oath-make` the host's Rust |
| R8 | Reset the progress docs ("slash and burn"). | Done in the cut |
| R9 | No Zig anywhere in Oath (Joshua, 2026-10-13). `pkg:cc` and the `dev` profile were removed. | Done |
| R10 | Keep the pinned 7.3-rc1 kernel for now. | Decided 2026-10-13 |
| R11 | The system toolchain is Clang/LLVM (clang, lld, compiler-rt, libunwind, libc++) on musl, built from source: stage 0 by the build host's gcc, stage 1 (`pkg:llvm`) by stage 0. One toolchain for every target; mesa needs LLVM anyway. | Decided 2026-10-14; superseded by R17 |
| R12 | Time comes from pool.ntp.org (`0`–`3.pool.ntp.org`): busybox `ntpd` as `svc:time` in the `base` seed. | Decided by Joshua 2026-10-15; done; the daemon is chrony since 2026-10-17 (R18) |
| R13 | An origin carries each package's source beside it: `cargo make publish` copies the plan that built a pack and the `-src` pack that plan pins, for every published package, GPL or not. | Decided by Joshua 2026-10-16; done |
| R14 | Spec and status split: every kind that converges keeps the spec it last applied (`applied.json`), and drift and undo read only spec; status (`actual`) is observation only and lives on a tmpfs at `/oath/status`, rebuilt at every boot, outside every snapshot. History that must survive (generation records, a service's last-run record) stays on disk with the spec or the log. | Decided by Joshua 2026-10-16 (option "B then C"); done |
| R15 | The screen: when no desk is installed, the screen and keyboard (tty1) are a login shell as `home` (`svc:screen` in the `base` seed; root through `sudo`); a desk profile turns it off and owns the display. The serial console stays root. | Decided by Joshua 2026-10-16; done |
| R16 | The agent surface is `oath mcp`: an MCP server on stdin and stdout, run on each machine over SSH as `home` (`ssh -T home@host oath mcp`), each tool one `oath --json` verb (writes through `sudo`). Acceptance is a real model given only that server and a task (`cargo make check --agent`, opt-in, an API key in novus's environment, never committed). A resident agent stays out of core, as a payload. Spec: [specs/agent.md](specs/agent.md). | Decided by Joshua 2026-10-17; `oath mcp` and `check --agent` built 2026-10-17; done 2026-10-18 (phase 7) |
| R17 | One compiler and one libc: GCC, binutils and glibc, built from source (stage 0 by the build host's gcc, stage 1 by stage 0). Clang/LLVM and musl leave the system; mesa builds without LLVM (radeonsi and RADV compile with ACO). Programs link dynamically against glibc: its loader is `/lib/ld-linux-x86-64.so.2`, and `/lib` is a link farm of every linked package's libraries, as `/bin` is of its programs, on machines and in the build sandbox. No RPATHs and no `/lib64`; a vendor executable Oath packages gets its interpreter set to the `/lib` loader. Oath's own binaries (PID 1, `oath`, `sudo`, the logins) stay static, against glibc. Why: one C++ runtime (`libstdc++`, which vendor binaries load), glibc built as upstream builds it, vendor releases usable under R7, LLVM (the slowest build) gone. Cost: GPL and LGPL where LLVM and musl were permissive; no llvmpipe. | Decided by Joshua 2026-10-17; built 2026-10-17 (phase 8, slice 1) |
| R18 | No busybox: one GNU tool for each job. bash (also `/bin/sh`), coreutils, sed, grep, gawk, findutils, diffutils, patch, tar, gzip, xz, procps-ng, less and iproute2; dhcpcd is the DHCP client and chrony keeps time (`svc:time`, still pool.ntp.org, R12); vim is the editor, and iputils gives `ping`. `reboot`, `poweroff` and `halt` are PID 1's own binary, as `modprobe` is. Why: R17 made Oath a GNU system, upstream builds assume GNU tools, and each busybox applet duplicated one (awk beside gawk, wget beside curl, mount beside util-linux). | Decided by Joshua 2026-10-17; built 2026-10-17 (phase 8, slice 1) |
| R19 | Oath's compositor is River ported to C on wlroots and named rivulet: a hard fork of River 0.4.5 (upstream `f6d9617` and Oath's five patches), its source in `apps/rivulet/`, GPL-3.0-only like its origin. It serves the same protocols (river-window-management-v1 at version 4 and the other `river-*` protocols), so `sola-river` drives it unchanged. Upstream River and Zig leave the build (R9); `forks/river` goes when rivulet runs Sola. One Rust process that merges the compositor and `sola-river` (`sola-wm`, on Smithay) is a possible later goal, built in Sola, not Oath. Why: Sola uses a small part of River's protocol, and wlroots keeps Xwayland, capture and the drivers' edge cases someone else's problem; a Smithay rewrite of River's protocol would take Smithay's gaps and River's complexity at once. | Decided by Joshua 2026-10-18 |
| R20 | The desk's seat is seatd: `svc:seatd` runs as root and gives `home` the GPU's display node and the input devices through libseat (`-u home -g home`; rivulet sets `LIBSEAT_BACKEND=seatd`). No logind, no udevd: PID 1 opens the render nodes to everyone (GL clients open them themselves) and passes kernel uevents on to libudev-zero (netlink group 4). The `desk` seed profile extends `base` with `svc:seatd`, `svc:rivulet` (as `home`) and their packages, and turns `svc:screen` off. Why: it is what worked on the pre-cut desk, and seatd revokes devices on a VT switch where libseat's noop backend cannot. | Decided by Joshua 2026-10-18 |
| R21 | Sola on the desk is its session (bus, call, the River window manager, shell, session manager) as `svc:sola-*` under PID 1, with three apps: the terminal, Workspaces (added by Joshua 2026-10-19) and the browser. Other Sola apps wait. Sola is built from oath-sola (`forks/sola`, kept merged with Sola `master`) by `pkg:rust`, offline against `pkg:sola-crates` (`cargo vendor` of its Cargo.lock); the browser runs on CEF, the vendor's binary release (R7), with its library stack built here. | Decided by Joshua 2026-10-18 |
| R22 | The desk's fonts are Noto (UI, serif, CJK, colour emoji, math and the other scripts) with Iosevka Term Slab first for monospace; SF Pro cannot ship (Apple licenses it only for its own platforms). Sound is PipeWire: `svc:pipewire`, `svc:wireplumber` and `svc:pipewire-pulse` as `home`, beside a session bus (`svc:dbus-session`); PID 1 gives `/dev/snd` to `home`. The desk carries what builds Sola on the machine (rust, gcc, pkgconf, cmake, meson, ninja, make, perl, python, git; `PKG_CONFIG_PATH` in `host:local` env), and `svc:sola-kvm` listens on UDP 4242 so novus's `sola-kvm server` drives canto. | Decided by Joshua 2026-10-18 |
| R23 | A package whose licence does not allow redistribution ships as a **recipe**, never a pack: its plan and its pinned vendor source are in every catalog (and on an origin), and each machine fetches the vendor's file itself and makes the pack with `oath build plan:<name>`. `pkgs/<name>/pkg.toml` says why (`recipe = "…"`); a recipe has one source and builds only with packs the base profile ships; images and `cargo make publish` carry its plan, not its pack; the seed pins the product hash the build host got, so a machine's build must match it. The first is `pkg:claude-code`: Claude Code from Anthropic's `stable` channel (2.1.292), pinned by version and sha256 and checked on the build host against the manifest Anthropic signs (`pkgs/keys/anthropic.asc`, `31DD DE24 DDFA B679 F42D 7BD2 BAA9 29FF 1A7E CACE`); its loader name is rewritten in place to `/lib`'s, and `bin/claude` runs it with `DISABLE_UPDATES=1`, so a new version is a new pin. It stands alone: in the base profile, absent, and Sola Workspaces does not depend on it. | Decided by Joshua 2026-10-19 |

## Standing

- **Shape.** x86_64. Linux kernel, own userspace, own PID 1 in Rust.
  glibc, dynamically linked, the only libc; GCC the only compiler (R17).
  MIT.
- **Catalog.** `/oath`, objects `kind:name`, pretty JSON
  `desired` / `actual` / `meta` (+ `applied` for svc), JSON Schema
  per kind with Markdown prose, `INDEX.md` at the root. Kinds:
  `host`, `svc`, `snap`, `pkg`, `net`, `ssh`, `dev`, `plan`. No new
  kind without a decision here.
- **One admin surface.** `oath` verbs, text by default, `--json`
  the same facts. Agents do not edit `/etc`.
- **Confirm class.** Halt, wipe and boot-generation changes need
  `--confirm`; agents pass it only when the owner asked.
- **Generations.** btrfs `@` with sibling `@gen-N`; `snap:current`.
  Undo is the snapshot. qcow snapshots are host debugging only.
- **Principal.** Serial console is root (break-glass). SSH, the screen
  (tty1, when no desk owns it; R15) and daily use are Unix user `home`
  (uid 1, `HOME=/home`). The agent is not a third user. Root SSH is
  off (`dropbear -w`).
- **Packages.** Store `/oath/store/pkg/<name>/<hash>/`, hash is
  SHA-256 of `oath-tree-v1`; `/bin` is a symlink farm; `desired.hash`
  pins and apply verifies. Base packages are not removable. `needs`
  is `{id, hash}`; apply refuses a broken graph. `description` /
  `home` are prose. `components.json` is in the tree hash;
  advisories are hash-keyed files; the guest never calls OSV.
  `pkg.url` / `{origin}/pkg/<name>/<hash>.tar` fetch; no repository
  kind. Builds are `plan` objects, netless sandbox, pinned inputs.
- **Network and SSH.** `net:net0` is the one NIC (dhcp by default).
  Host keys live under `/oath/ssh/`; owner public keys in `ssh:local`.
  No baked private keys.
- **Devices.** `dev` is inventory; no udev; the catalog does not load
  modules.
- **Time.** Kernel, logs and catalog are UTC. `host:local.timezone`
  is exported as `TZ` to services and logins; `/etc/localtime` is not
  truth.
- **Boot.** ESP keeps the current boot plus the last five archives
  (`cargo make esp`, run on the Oath machine); `oath-efi` is the
  boot menu and mark.
- **Driver patches first.** A generation-gap GPU bug is fixed in
  `pkgs/linux/patches/`, gated by chip family, not worked around
  in userspace.

## Superseded

Borrowed prebuilts in phase 1 (D8 2026-08-27) → R7. Sola/River on
Oath (T21–T29, T36, T40), seat device rules (T22, T31 parts), Grok
and Thoxa packs (T30, `pkg:thoxa`), the glibc runtime pack, T35's
glibc-default `cc` → removed with the desk; returns as native
packages. T37, T39 → R2. Package hosting on Wicket (T20 origin
default) → no default origin. T33 backup → kept as a script and a
canto overlay, not a seed default.
