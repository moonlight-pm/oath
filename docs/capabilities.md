# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: [`manual/`](manual/) — current behavior.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-08-31

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
| boot | Kernel + init + QEMU | partial | [plan](plans/2026-08-27-qemu-skeleton-plan.md) | `cargo make run` / `probe` | borrowed kernel; River + Sola session as `svc` | [qemu](manual/qemu.md) |
| pkg | Package objects | partial | [freeze](specs/2026-08-30-pkg-hosting.md) | QEMU probe + fetchme | T20 identity: `pkg.url`, peer Oath host as origin; no guest store export, hash, deps, or versions | [packages](manual/packages.md) |
| net | Network objects | partial | [freeze](specs/2026-08-30-ssh-and-dhcp.md) | QEMU probe ping + SSH | dhcp implemented; LAN bridge opt-in (`OATH_BRIDGE`) | [network](manual/network.md) |
| dev | Device objects | partial | [freeze](specs/2026-08-30-libinput.md) | QEMU probe | vda/net0/ttyS0/card0/kbd0/mouse0; no module loader; no udev | [devices](manual/devices.md) |
| ssh | Root SSH via catalog keys | partial | [freeze](specs/2026-08-30-ssh-and-dhcp.md) | probe login/deny/undo/reboot | dropbear; no baked private key; no second Unix user | [ssh](manual/ssh.md) |
| agent | Agent runtime as a system component | idea | — | — | serial or SSH client | no |
| update | Base-image updates | idea | — | — | generations are the primitive | no |
| install | Installer to a disk | idea | — | — | not started | no |
| sola | Sola session on Oath | partial | [freeze](specs/2026-08-31-sola-terminal.md) | QEMU probe sola.bus / sola.call / sola.bridge / sola.shell / sola.session / sola.terminal / sola.tmux_session / sola.mono_font | session stack + LaunchApp owner packed (flower + McMojave cursor); gtk 1280×800 1:1 (preferred virtio-gpu mode, not max CVT); menubar panels Frame to the card (not full-output software GL); Quit Sola is clean exit (`on-failure`); one `pkg:sola` blob including `sola-terminal` + tmux (`libresolv` + C.UTF-8 locale-archive) + JetBrains Mono; Sola developed on Nix (T24); no nested PM; no udevd; other kit apps out; no dbus; software GL; launcher/switcher still full-output | [services](manual/services.md) |
