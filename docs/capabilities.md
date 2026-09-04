# Capabilities — as-built progress

**Purpose:** What exists vs what remains for each product capability.
Target design: [specs/](specs/). Session priority: root [CURRENT.md](../CURRENT.md).
Product docs: [`manual/`](manual/) — current behavior.

**Update this file in the same change** whenever status or gaps change.

**Status vocabulary:** `shipped` · `partial` · `spec’d` · `planned` · `idea`

**As of:** 2026-09-03

---

## Founding band

| ID | Capability | Status | Spec / plan | Dogfood | Gaps | Manual |
|----|------------|--------|-------------|---------|------|--------|
| charter | Principles + independence + AI-first thesis | spec’d | [CURRENT locks](../CURRENT.md) | — | none for v0 | no |
| catalog | Live INDEX + schemas | partial | [freeze](specs/2026-08-27-catalog-and-oath-surface.md) | QEMU probe | INDEX generated; `oath-tel` + `/oath/log` | [catalog](manual/catalog.md) |
| objects | Typed object store | partial | freeze | QEMU | host/svc/snap/pkg/net/ssh/dev | [catalog](manual/catalog.md) |
| oath-cli | `oath` verbs | partial | freeze | QEMU probe | MCP later | [using](manual/using.md) |
| snap | generations; apply / undo | partial | freeze | probe (sibling `@gen-N`, reboot) | boot-generation pick still confirm-only; no bootloader menu; off-box send is T33 (not implemented) | [generations](manual/generations.md) |
| svc | Own supervisor | partial | freeze | QEMU: hold+sshd+seatd+river+sola; canto: sshd + river/Sola as `home` (serial off); pipewire trio seeded | no Requires-style hard deps; canto `svc:serial` off (no UART); live canto pipewire is hand-started as `home` this boot (ESP initrd still old PID 1) | [services](manual/services.md) |
| boot | Kernel + init + QEMU | partial | [plan](plans/2026-08-27-qemu-skeleton-plan.md) | `cargo make run` / `probe`; canto EFI splash | borrowed kernel. Layers: `oath-efi` (GOP mark) if EFI; defer KMS that steal firmware fb until River; River black until Sola. ALSA HDA (`snd-hda-intel`, HDMI, Cirrus) packed and loaded with the KMS pass. Not universal — QEMU `-kernel` has no EFI splash; virtio-gpu is not deferred. Gaps: Apple firmware still first; SI/Pitcairn modeset flash when KMS binds (no seamless handoff); canto ESP initrd not yet rebuilt with snd + new PID 1 | [qemu](manual/qemu.md) |
| pkg | Package objects | partial | [freeze](specs/2026-09-03-pkg-pack-identity.md) | QEMU probe + fetchme; canto `pkg:grok` / `pkg:git` / `pkg:curl` / `pkg:pipewire` / `pkg:thoxa` (hand-copied store this boot) | T20 identity: `pkg.url`, peer Oath host as origin; **T30** `pkg:grok` packed (catalog owns ELF; Grok does not self-update); `pkg:git` + `pkg:curl` + `pkg:pipewire` + `pkg:thoxa` packed; **T32** pack identity spec’d (content hash, `/oath/store/pkg/<name>/<hash>/`, `desired.hash` pin, no recipe language) — not implemented; as-built store is still `/oath/store/pkg/<name>/`; no guest store export, deps, or url-refetch | [packages](manual/packages.md) |
| net | Network objects | partial | [freeze](specs/2026-08-30-ssh-and-dhcp.md) | QEMU probe ping + SSH; canto dhcp 10.0.0.3 | dhcp implemented; wait-for-carrier before rename when several NICs; LAN bridge opt-in (`OATH_BRIDGE`) | [network](manual/network.md) |
| dev | Device objects | partial | [freeze](specs/2026-08-30-libinput.md) | QEMU probe | vda/net0/ttyS0/card0/kbd0/mouse0; no module loader; no udev | [devices](manual/devices.md) |
| ssh | Home SSH via catalog keys | partial | [freeze](specs/2026-09-02-seat-home.md) | QEMU probe login/deny/sudo as `home`; **canto `ssh` / `scp` / `sftp` as `home@canto` (uid 1); login shell `/bin/thoxa`** | dropbear `-w`; uid 1; sudo ALL no password; groups `root`+`home`; interactive PTY (`ptmxmode=0666`); no baked private key; `pkg:dropbear` includes `/bin/scp` + `/bin/sftp-server` (canto live 2026-09-03; next `cargo make build` for QEMU). Editor is busybox `/bin/vi`. Home login shell is `/bin/thoxa` (must be in `/etc/shells` or dropbear looks like publickey denied) | [ssh](manual/ssh.md) |
| agent | Agent runtime as a system component | idea | — | — | serial or SSH client | no |
| update | Base-image updates | idea | — | — | generations are the primitive; off-box copy is T33 | no |
| backup | Off-box generation (NFS) | spec’d | [freeze](specs/2026-09-03-backup-nfs.md) | none | T33: one NFS send stream, overwrite, explicit `svc:backup`; no NFS client in image; no helper; no restore-in-installer; no hourly/retention | no |
| install | Installer to a disk | partial | [freeze](specs/2026-08-31-metal-canto.md) | QEMU-EFI rehearsal + canto T31 reinstall | `cargo make install --target --disk --confirm`; `--usb` EFI ramdisk. `oath-efi` reads `loader/entries/oath.conf` (not systemd-boot oneshot). Post-install SSH is `home`. ESP initrd `/init` stays PID 1 after chroot. Gaps: kexec not used on this Apple; no gen picker | [install](manual/install.md) |
| sola | Sola session on Oath | partial | [freeze](specs/2026-09-02-seat-home.md) | QEMU session probe (stack as `home` uid 1); canto T31 SSH + River/Sola as `home` (uid 1, amdgpu DP-10, gles2/radeonsi); oath-sola packed live (LED graphs rastered); volume chip + Built-in Audio this boot; **sola-kvm listen** on canto (UDP 4242, virtual pointer; CLIP1 text + PNG) | T31 QEMU + canto metal: graphical stack as `home`; kit ELFs in `pkg:sola` packed (flower Restart/Shutdown via `oath apply --confirm`; Super+Tab counts, notify pile, spectrum, rounded floats, browser omnibox/devtools; **`/bin/sola-kvm`**); virtio-only SW cursor + pixman + `LIBGL_ALWAYS_SOFTWARE`; metal: gles2/radeonsi; `pkg:grok` / `pkg:git` / `pkg:curl` / `pkg:pipewire` / `pkg:thoxa` packed (rustc still host-side); wrapper/mail/spotify/other kit apps out; no dbus-daemon; PipeWire HDMI not udev-enumerated (PCH analog pinned); live pipewire + sola-kvm daemons are seat processes this boot, not yet ESP-initrd svcs | [services](manual/services.md) |
