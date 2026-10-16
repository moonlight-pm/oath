# Decisions

What has been decided, and what still needs Joshua. Agents ask before
acting on anything under **Open**. Replaces `open-questions.md`
(history in git, tag `t37-t39-final`).

## Open — ask the human

- **Steam + Minecraft desk.** Which machine, and when. Not before the
  native desk exists.
- **GPL source on origins.** `cargo make publish` puts GPL binaries
  (busybox, btrfs, util-linux, dosfstools) on an origin without their
  source. Should it also publish each package's `-src` pack and plan
  (the exact sources, patches and build script) beside it? Until
  decided, publish to private origins only. Joshua (2026-10-16) would
  like permissive alternatives to the GPL packages some day; not now.

## 2026-10-13 review (briefing D1–D8)

| ID | Decision | Status |
|----|----------|--------|
| R1 | Wicket leaves the core. Products wait for Oath to mature, then bundle themselves with the package standard. | Done in the cut |
| R2 | Delete T37 (Steam, gamescope, sola-arcade) and T39 (Omarchy/Hyprland). Tag `t37-t39-final`. A Steam + Minecraft desk is a future requirement. | Done in the cut |
| R3 | The seed is data: profiles `base` and `dev`; machine facts are install overlays. | Done in the cut |
| R4 | Every catalog writer takes an exclusive file lock (`flock`) and writes temp-file-then-rename; PID 1 reads only `applied`. | Decided 2026-10-13; phase 3 |
| R5 | Delete the probe; add a headless QEMU harness that gates installs. | Probe deleted; harness next |
| R6 | One kernel: own-built, sha256-pinned, vermagic-checked. No Nix kernel. | Done in the cut |
| R7 | Native packages from the outset: built from source by an Oath plan, or the vendor's release pinned by sha256. Never another distro's packages. Build-host compilers bootstrap only. | Done 2026-10-14 (phase 5); `pkg:llvm` and the kernel are plan products since 2026-10-15, and the Oath binaries (built by the vendor's Rust, `pkg:rust`) since 2026-10-16; only stage 0 uses the host's gcc, and only `oath-make` the host's Rust |
| R8 | Reset the progress docs ("slash and burn"). | Done in the cut |
| R9 | No Zig anywhere in Oath (Joshua, 2026-10-13). `pkg:cc` and the `dev` profile were removed. | Done |
| R10 | Keep the pinned 7.3-rc1 kernel for now. | Decided 2026-10-13 |
| R11 | The system toolchain is Clang/LLVM (clang, lld, compiler-rt, libunwind, libc++) on musl, built from source: stage 0 by the build host's gcc, stage 1 (`pkg:llvm`) by stage 0. One toolchain for every target; mesa needs LLVM anyway. | Decided 2026-10-14 |
| R12 | Time comes from pool.ntp.org (`0`–`3.pool.ntp.org`): busybox `ntpd` as `svc:time` in the `base` seed. | Decided by Joshua 2026-10-15; done |

## Standing

- **Shape.** x86_64. Linux kernel, own userspace, own PID 1 in Rust.
  musl base; never two libcs in one process. MIT.
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
- **Principal.** Serial console is root (break-glass). SSH and daily
  use are Unix user `home` (uid 1, `HOME=/home`). The agent is not a
  third user. Root SSH is off (`dropbear -w`).
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
