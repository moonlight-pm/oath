# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: [`manual/`](manual/) — current behavior.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-09-02

---

## Founding band

| ID | Capability | Status | Spec / plan | Dogfood | Gaps | Manual |
|----|------------|--------|-------------|---------|------|--------|
| charter | Principles + independence + AI-first thesis | spec’d | [CURRENT locks](../CURRENT.md) | — | none for v0 | no |
| catalog | Live INDEX + schemas | partial | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | QEMU probe | INDEX generated; `oath-tel` + `/oath/log` | [catalog](manual/catalog.md) |
| objects | Typed object store | partial | freeze | QEMU | host/svc/snap/pkg/net/ssh/dev | [catalog](manual/catalog.md) |
| oath-cli | `oath` verbs | partial | freeze | QEMU probe | MCP later | [using](manual/using.md) |
| snap | generations; apply / undo | partial | freeze | probe (sibling `@gen-N`, reboot) | boot-generation pick still confirm-only; no bootloader menu | [generations](manual/generations.md) |
| svc | Own supervisor | partial | freeze | hold + sshd + seatd + river + sola session | do not disable serial; no Requires-style hard deps | [services](manual/services.md) |
| boot | Kernel + init + QEMU | partial | [plan](plans/2026-08-27-qemu-skeleton-plan.md) | `cargo make run` / `probe`; canto EFI splash | borrowed kernel. Layers: `oath-efi` (GOP mark) if EFI; defer KMS that steal firmware fb until River; River black until Sola. Not universal — QEMU `-kernel` has no EFI splash; virtio-gpu is not deferred. Gaps: Apple firmware still first; SI/Pitcairn modeset flash when KMS binds (no seamless handoff) | [qemu](manual/qemu.md) |
| pkg | Package objects | partial | [freeze](specs/2026-09-02-pkg-grok.md) | QEMU probe + fetchme | T20 identity: `pkg.url`, peer Oath host as origin; **T30** `pkg:grok` identity (catalog owns ELF; Grok does not self-update); ELF not packed; no guest store export, hash, deps, versions, or url-refetch | [packages](manual/packages.md) |
| net | Network objects | partial | [freeze](specs/2026-08-30-ssh-and-dhcp.md) | QEMU probe ping + SSH; canto dhcp 10.0.0.3 | dhcp implemented; wait-for-carrier before rename when several NICs; LAN bridge opt-in (`OATH_BRIDGE`) | [network](manual/network.md) |
| dev | Device objects | partial | [freeze](specs/2026-08-30-libinput.md) | QEMU probe | vda/net0/ttyS0/card0/kbd0/mouse0; no module loader; no udev | [devices](manual/devices.md) |
| ssh | Home SSH via catalog keys | partial | [freeze](specs/2026-09-02-seat-home.md) | QEMU probe login/deny/sudo/undo/reboot as `home` | dropbear `-w`; `home` only; serial root; sudo ALL no password; no baked private key; canto still root until T31 install | [ssh](manual/ssh.md) |
| agent | Agent runtime as a system component | idea | — | — | serial or SSH client | no |
| update | Base-image updates | idea | — | — | generations are the primitive | no |
| install | Installer to a disk | partial | [freeze](specs/2026-08-31-metal-canto.md) | QEMU-EFI rehearsal + canto SSH | `cargo make install --target --disk --confirm`; `--qemu` OVMF rehearsal; `--usb` EFI ramdisk stick. Canto entered via USB/EFI oneshot (kexec left Broadcom tg3 down). GPT ESP+btrfs `@`, systemd-boot. Gaps: kexec not used on this Apple; no gen picker | [install](manual/install.md) |
| sola | Sola session on Oath | partial | [freeze](specs/2026-09-02-seat-home.md) | QEMU session probe (stack as `home` uid 1000); canto HDMI + live browser + **Workspaces** (still root until T31 install) | T31 QEMU: graphical stack as `home`; session stack packed from Sola master (Super+K card Frame); kit ELFs in `pkg:sola`: terminal, browser (CEF), **workspaces + solactl**; kit seed fonts; virtio-only SW cursor + `LIBGL_ALWAYS_SOFTWARE`; canto hardware cursors; no git/grok/rustc on the guest; no radeonsi; wrapper/mail/other kit apps out; no dbus-daemon | [services](manual/services.md) |
