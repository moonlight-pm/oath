# Architecture

**Role:** as-built system map (what the code and runtime look like **now**).
**Not** the place for multi-feature roadmaps or session priority.

| Concern | Document |
|---------|----------|
| Capability maturity | [capabilities.md](capabilities.md) |
| Target design freezes | [specs/](specs/) |
| Session priority + dogfood | Root [CURRENT.md](../CURRENT.md) |
| How docs fit together | [progress-model.md](progress-model.md) |
| Product docs | [manual/](manual/) — current operator manual |
| Interactive overview | [architecture.html](architecture.html) (`cargo make map`) |

---

## Interactive overview

The 10-node as-built picture is [architecture.html](architecture.html)
(Archify, showcase). Open it:

```sh
cargo make map                 # xdg-open the HTML
cargo make map --print         # path only
cargo make architecture        # alias
```

Public copy (Wicket forge workload): **https://oath.wicket.cloud/** (map at `/map`).
Pack origin: **https://store.oath.wicket.cloud/** (`pkg/<name>/<hash>.tar`).
Refresh the map site from this tree: copy `docs/architecture.html` into
Wicket `extras/oath/site/architecture.html`, rebuild `oath-site`, apply
(see Wicket `extras/oath/README.md`). Refresh packs with `cargo make
publish` into `extras/oath-store/site`.

Prose below is the detailed map. The HTML is the overview (admin surface,
boot/PID 1, catalog objects, seat session). Do not hand-edit the HTML.

**Source spec:** [architecture.archify.json](architecture.archify.json).

### Keeping it current

Same authority as this file: as-built, not a roadmap. When the system map
changes (processes, crates, boot path, catalog kinds, seat/session):

1. Update this prose map first. It is the evidence.
2. Edit `architecture.archify.json` so nodes, edges, `components[].sources`,
   and cards match. Keep **at most 12** nodes. Do not invent topology.
3. Set `meta.repository.revision` to a 40-character commit whose blobs match
   those `sources` paths and line ranges (`git rev-parse HEAD` after the cited
   code exists).
4. Rebuild the HTML (needs Node, and Archify at `OATH_ARCHIFY` or
   `~/.grok/skills/archify`):

   ```sh
   cargo make map --render
   ```

   Showcase must pass (9/9 checks, 0 errors, 0 warnings) or `--render` fails
   and leaves the previous HTML.
5. Commit **JSON + HTML + this file** in the same change as the code.

`--render` does not invent a new diagram. It only delivers the spec already
in the tree.

---

## As-built (2026-09-11)

QEMU x86_64 appliance. Serial, SSH, and (if DISPLAY) a gtk window.

```
QEMU -kernel bzImage -initrd initrd.gz -netdev user -device virtio-net-pci
-device virtio-gpu-pci,xres=1280,yres=800 -display gtk,zoom-to-fit=off (or none) -drive virtio qcow2
  kernel (vanilla kernel.org + Oath fragment + image/linux-patches; 7.3 for GFX6) + initramfs
    (Pitcairn amdgpu/*.bin in initrd /lib/firmware)
    /init = oath-init
    loads virtio_blk, btrfs, virtio-gpu, evdev, virtio_input, virtio_net, …
    mounts /dev/vda subvol=@ , chroot
    mounts subvolid=0 at /oath/run/fs
  disk (btrfs)
    @            live root
    @gen-N       readonly sibling generations (catalog undo)
    @boot-N      read-write snapshot at ESP rotate (firmware menu)
    /init (ESP initrd)     PID 1; chroot into @ and keeps running
    /lib/oath/init         same ELF on disk (not exec'd after chroot)
    /lib/oath/serial-login
    /lib/oath/sudo         setuid; /bin/sudo
    /bin/*                 symlink farm into /oath/store/pkg/<name>/<hash>/bin/
    /home                  seat home (Unix user `home`, uid 1)
    /oath/                 catalog
    /oath/store/pkg/{busybox,btrfs,oath,dropbear,glibc,river,hyprland,quickshell,omarchy,sola,grok,git,curl,pipewire,bluez,thoxa,cc,rustc,cmake,pkg-config,bash,foot,grim,xwayland,gamescope,mesa,steam,hello,fetchme}/<hash>/
    net0               virtio-net (QEMU user or OATH_BRIDGE)
    /dev/dri/card0     virtio-gpu (dev:card0)
    /dev/input/event*  virtio keyboard + mouse (dev:kbd0, dev:mouse0)
    /oath/ssh/         dropbear host keys (generated)
    dropbear           svc:sshd, keys from ssh:local; musl OpenSSH /bin/ssh +
                       /bin/scp + /bin/sftp-server (SFTPSERVER_PATH=/bin/sftp-server)
    seatd              svc:seatd (DRM seat)
    hyprland           pkg:hyprland + svc:hyprland (session=omarchy;
                       NO_SYSTEMD; libudev-zero; wants seatd; canto DP-10
                       1920×1080 Hyprland 0.52.2; `/lib/oath/hyprland-boot.conf`
                       tiling + Super+Return/Space/K/Ctrl+C; seed enabled=false)
    quickshell         pkg:quickshell + svc:omarchy-shell (Omarchy bar +
                       menu; wants svc:hyprland; seed enabled=false)
    omarchy            pkg:omarchy tree at $OMARCHY_PATH
                       (/oath/store/pkg/omarchy/live); pack-time adapt
                       (uwsm-app / gtk-launch / xdg-terminal-exec / jq
                       shims; omarchy-pkg-* → /lib/oath/omarchy-pkg-oath);
                       share/fonts = JetBrainsMono NF + Liberation +
                       Noto Color Emoji; share/icons/Yaru XCursor;
                       fontconfig in pkg:quickshell remaps
                       `JetBrainsMono Nerd Font` → `JetBrainsMono NF`
    omarchy-menu-toggle /lib/oath/omarchy-menu-toggle (Super+Space → qs ipc)
    omarchy-menu-keybindings Super+K → hyprctl binds + omarchy-menu-select
                       (Oath select shim; no perl/gawk)
    grim               pkg:grim — grim + slurp + hyprpicker + wl-copy + jq
                       (Super+Ctrl+C Capture menu; Print → screenshot)
    river              svc:river as `home` (glibc, libudev-zero, socket /run/user/1;
                       gles2/radeonsi on real KMS, pixman on virtio; hardware
                       cursors unless a DRM card is virtio; packed
                       `+xwayland`, `WLR_XWAYLAND=/bin/Xwayland`; Super+Q
                       SIGTERMs X11 class `steam` after WM_DELETE)
    sola-bus/call      svc:sola-bus / svc:sola-call as `home` (sockets /run/user/1)
    sola-river         svc:sola-river (bridge, not the compositor;
                       `/bin` wrapper retries while `pidof river` is empty)
    sola-shell         svc:sola-shell (iced menubar; wgpu/gl; llvmpipe forced
                       only on virtio KMS; McMojave; LED graphs are RGBA images;
                       same compositor-retry wrapper as the bridge)
    sola-session       svc:sola-session (LaunchApp; direct spawn)
    pipewire           pkg:pipewire + seat svcs (pipewire, wireplumber,
                       pipewire-pulse) as `home`; `/run/user/1/pipewire-0`;
                       ALSA PCH analog pinned as Built-in Audio (no udevd
                       card enum). Canto cards: Intel HDA PCH + two HDMI.
    bluez              pkg:bluez; svc:dbus (system bus) + svc:bluetoothd;
                       `/run/dbus/system_bus_socket` + zbus
                       `/var/run/dbus/…` symlink; canto Apple BCM20702
                       `hci0` (`05ac:828d`); initrd packs btusb
    sola-terminal      /bin/sola-terminal (kit app in pkg:sola; tmux helper)
    sola-browser       /bin/sola-browser (kit app in pkg:sola; CEF under cef/)
    sola-workspaces    /bin/sola-workspaces (kit app in pkg:sola; tmux sola-ws)
    solactl            /bin/solactl (call-plane CLI in pkg:sola)
    xdg-open           /bin/xdg-open and /bin/open (solactl open →
                       sola-browser, or sola-paint for images;
                       x-www-browser same; not xdg-utils)
    sola-paint         /bin/sola-paint (image viewer/editor; Omarchy
                       starts sola-bus on demand via /lib/oath/ensure-sola-bus)
    sola-kvm           svc:sola-kvm listen as `home` (UDP 4242; virtual
                       pointer on River or Hyprland; shared seat)
    pkg:thoxa          `/bin/thoxa` (glibc; session REPL is the `home` login shell)
    pkg:cc             `/bin/cc` (Zig; gnu default, musl-cc for Oath ELFs;
                       `zig-gnu-cc.sh` drops rustc `-fuse-ld=lld` / cc-rs `--target=`)
    pkg:rustc          `/bin/rustc` `/bin/cargo` (1.98.1 gnu host + musl std
                       + rust-std `x86_64-unknown-uefi` for `oath-efi`)
    pkg:cmake          `/bin/cmake` `/bin/ninja`
    pkg:pkg-config     `/bin/pkg-config` (empty .pc farm)
    pkg:bash           `/bin/bash` (GNU 5.2.15 static musl)
    pkg:xwayland       `/bin/Xwayland` (Debian 24.1.13; session Steam is
                       River `+xwayland` `:0`; `-glamor off` on SI —
                       river radeonsi SIGBUS’d on tiled BOs)
    pkg:gamescope      `/bin/gamescope` (Arcade Play nest on GPUs with
                       Vulkan WSI DRM modifiers; seed `present: false`;
                       apply refuses on AMD GFX6–8 / virtio)
    pkg:mesa           64-bit GLX/GL/EGL + Vulkan WSI (Debian mesa 26.2.1);
                       `/bin/vulkaninfo`;
                       DRI `libdril`→radeonsi; 32-bit RADV in `lib32`
                       plus `libdisplay-info.so.3` + `libxml2.so.16` +
                       `libwayland-client` 1.26 (`wl_fixes`); ICD DT_RPATH
    pkg:steam          `/bin/steam` wrapper (session X11 on River, not
                       gamescope); 32-bit loader in lib32 + 64-bit
                       steamrt3 SONAMEs in lib64; host `_v2-entry-point`
                       at `libexec/pv-host` (no bwrap);
                       `libexec/oath-lsof` (`/bin/lsof` wrapper);
                       `lib64/liboath-glclass.so` (64-bit libGL redirect);
                       `STEAM_FRAME_FORCE_CLOSE=1` on `:0`; CEF `--disable-gpu`;
                       live nodes `/usr/bin/env`, `/lib64/ld-linux-x86-64.so.2`,
                       `/lib/ld-linux.so.2`, `/etc/ssl/certs`, `/bin/lsof`,
                       `/usr/share/vulkan/icd.d/radeon_icd.{x86_64,i686}.json`
                       (not the /bin farm)
    sola-arcade        kit app in pkg:sola; `/bin` link only when the
                       GPU has DRM modifiers (not on canto Pitcairn)
    pkg:sola fonts     SF Pro Text + Iosevka Term Slab (Inter / JetBrains Mono fallbacks)
    backup-send        /lib/oath/backup-send (T33 NFS `btrfs send`)
    backup-daily       /lib/oath/backup-daily (04:00 Mountain loop; seed off)
    with-seat-tz       /lib/oath/with-seat-tz (T34 display TZ for seat)
    /sbin/init -> ../lib/oath/init
```

PID 1 is the initrd `/init` (stays PID 1 after chroot — that chroot is
why `CLONE_NEWUSER` is EPERM even as root). Mount proc/sys/dev/pts
(`ptmxmode=0666`), tmpfs `/tmp` `/dev/shm` `/run`, cgroup2; `/dev/fd` →
`/proc/self/fd` (bash process substitution; Omarchy plugin scan);
`lo` up with
`127.0.0.1` + `::1` (next image; canto this boot by hand); hostname +
`host:local.env`; seat `TZ` from `host:local.timezone`; load ethernet; **converge** `net:net0` (dhcp) + `ssh:local`;
sshd; then amdgpu + ALSA HDA (snd deferred with KMS); wait for
`/dev/dri` + `/dev/input` and chown them `0660` root:`home` (same for
`/dev/snd` when the nodes exist); then remaining `svc:*` (River/Sola
and pipewire as `home`).
Socket `/oath/run/init.sock`. `host:local.session` is `sola` (default)
or `omarchy` (T39); PID 1 starts only that desk’s compositor. Stopping
River or Hyprland waits up to 3 s for the DRM owner to exit before
starting the other.
Seeded services: `svc:serial`, `svc:hold`, `svc:sshd`, `svc:seatd`, `svc:river`,
`svc:hyprland` (seed off), `svc:omarchy-shell` (seed off),
`svc:sola-bus`, `svc:sola-call`, `svc:sola-river`, `svc:sola-shell`,
`svc:sola-session`, `svc:sola-kvm`, `svc:pipewire`, `svc:wireplumber`,
`svc:pipewire-pulse`, `svc:dbus`, `svc:bluetoothd`, `svc:backup` (04:00 Mountain NFS send; seed off).

`oath apply` snapshots live `@` to sibling `@gen-N` under `/oath/run/fs`
(btrfs top-level). Undo restores catalog documents (including `store/`)
from that generation, not `/oath/run`. Fallback: copy the catalog tree
when the top-level is not mounted. Off-box: `svc:backup` runs
`/lib/oath/backup-daily` (04:00 Mountain in-process TZ; system clock
UTC) which calls `/lib/oath/backup-send` (T33).

Telemetry: guest lines `oath-tel {json}` on stderr and `/oath/log/*.jsonl`.
`oath apply` on `pkg:*` creates or removes `/bin` symlinks into
`/oath/store/pkg/<name>/<hash>/bin/`. Undo restores `store/` with the catalog
then converges links.

Host runs live under `build/runs/<id>/` (`cargo make run` / `up` /
`start` / `probe`). `cargo make ssh` is hostfwd 2222.

**Metal install (T27):** `cargo make install --target user@host --disk
/dev/sda --confirm` (`--qemu` OVMF rehearsal; `--usb --disk /dev/sdd`
writes an EFI installer stick). Installer ramdisk: `oath.install=1`,
dropbear, no `switch_root`. Format GPT ESP + btrfs `@`. Copy packed
tree. Boot graphics is layered, not one path: EFI GOP splash
(`oath-efi` as `BOOTX64.EFI`) when firmware has GOP; PID 1 defers KMS
drivers that would kick a live firmware framebuffer (amdgpu/i915/…,
not virtio-gpu) until just before the session compositor. Sola: River
starts black until Sola paints. Omarchy: Hyprland + Quickshell bar.
USB installer still systemd-boot + tty0. QEMU `run`
is still `-kernel`. Metal `oath-efi` reads `loader/loader.conf` +
`loader/oath-boots` (timeout 5) and `loader/entries/oath.conf` plus
`oath-<id>.conf` archives under `/oath/boot/<id>/`. `oath.subvol=@`
or `@boot-N`. `cargo make esp --esp --confirm` rotates without a wipe.
Canto live kernel is vanilla **7.3.0-rc1 #4** (`image/linux.fragment`).
Last-5 archives are boot **14** (kvm-aware PID 1), 13, 12, 11, 10
(boot 9 pruned this rotate; boot 7 / 4 earlier). systemd-boot is
`BOOTX64` so the menu is visible. Ubuntu generic is not the product kernel.
Two Broadcom `tg3` ports; live cable is MAC
`00:3e:e1:cb:06:08` (renamed `net0`). kexec left that NIC down; EFI
oneshot / USB installer is the working entry. After boot, PID 1 waits
for carrier then dhcp. Dual Pitcairn amdgpu (`si_support=1`);
`/lib/oath/run-compositor` binds River to the connected DRM card
(`session=sola`). Canto live is **`session=omarchy`**: Hyprland on the
connected card. The graphical stack runs as Unix user `home`. SSH is
`home`; serial is root. sola-river picks the mode matching physical mm.

Workspace crates: `oath-core`, `oath`, `oath-init`, `oath-efi` (UEFI
splash), `oath-make` (host build CLI: `cargo make`). Artifacts in
`build/` (gitignored). Interactive overview: `cargo make map`
([architecture.html](architecture.html)).
Source forks under `forks/`: `river`, `wlroots`, `sola` (`oath-sola`).

**Target:**
[specs/2026-08-27-catalog-and-oath-surface.md](specs/2026-08-27-catalog-and-oath-surface.md) ·
[specs/2026-08-28-packages.md](specs/2026-08-28-packages.md) ·
[specs/2026-08-29-pkg-base.md](specs/2026-08-29-pkg-base.md) ·
[specs/2026-08-29-net.md](specs/2026-08-29-net.md) ·
[specs/2026-08-30-ssh-and-dhcp.md](specs/2026-08-30-ssh-and-dhcp.md) ·
[specs/2026-08-30-devices.md](specs/2026-08-30-devices.md) ·
[specs/2026-08-30-display.md](specs/2026-08-30-display.md) ·
[specs/2026-08-30-pkg-hosting.md](specs/2026-08-30-pkg-hosting.md) ·
[specs/2026-08-30-sola.md](specs/2026-08-30-sola.md) ·
[specs/2026-08-30-libinput.md](specs/2026-08-30-libinput.md) ·
[specs/2026-08-30-oath-sola.md](specs/2026-08-30-oath-sola.md) ·
[specs/2026-08-31-sola-dev.md](specs/2026-08-31-sola-dev.md) ·
[specs/2026-08-31-sola-session.md](specs/2026-08-31-sola-session.md) ·
[specs/2026-08-31-sola-terminal.md](specs/2026-08-31-sola-terminal.md) ·
[specs/2026-08-31-metal-canto.md](specs/2026-08-31-metal-canto.md) ·
[specs/2026-09-01-sola-browser.md](specs/2026-09-01-sola-browser.md) ·
[specs/2026-09-02-sola-workspaces.md](specs/2026-09-02-sola-workspaces.md) ·
[specs/2026-09-02-pkg-grok.md](specs/2026-09-02-pkg-grok.md) ·
[specs/2026-09-02-seat-home.md](specs/2026-09-02-seat-home.md) ·
[specs/2026-09-03-pkg-thoxa.md](specs/2026-09-03-pkg-thoxa.md) ·
[specs/2026-09-03-pkg-pack-identity.md](specs/2026-09-03-pkg-pack-identity.md) ·
[specs/2026-09-03-backup-nfs.md](specs/2026-09-03-backup-nfs.md) ·
[specs/2026-09-04-utc-clock.md](specs/2026-09-04-utc-clock.md) ·
[specs/2026-09-05-guest-toolchain.md](specs/2026-09-05-guest-toolchain.md) ·
[specs/2026-09-05-sola-kit-apps.md](specs/2026-09-05-sola-kit-apps.md) ·
[specs/2026-09-05-arcade-steam.md](specs/2026-09-05-arcade-steam.md) ·
[specs/2026-09-06-boot-generations.md](specs/2026-09-06-boot-generations.md) ·
[specs/2026-09-10-omarchy-session.md](specs/2026-09-10-omarchy-session.md)
