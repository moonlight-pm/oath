# The appliance

**Today’s product:** an x86_64 QEMU machine with a serial console, plus
a metal canary (canto). Installer: [install.md](install.md). Not a
desktop. On metal, `EFI/BOOT/BOOTX64.EFI` is `oath-efi`: it sets GOP
to native (1920×1080 when the firmware lists it), paints the white
Oath mark on black, then LoadImage’s the kernel. PID 1 leaves that
mark alone until it loads amdgpu immediately before River. QEMU
`run`/`probe` still boot `-kernel` (no EFI splash).

Build tools (kernel, busybox, qemu, musl cc) are borrowed on the **host**.
They are not the runtime identity.

## Build and run

Needs: Rust with `x86_64-unknown-linux-musl`, loop-mount as root (`sudo`
for packing the disk), btrfs-progs, qemu.

```sh
nix-shell          # optional: kernel, qemu, musl cc, busybox, btrfs
cargo make build
cargo make run            # interactive serial; gtk window if DISPLAY is set
cargo make run --build    # pack first, then run (same flag on up / start)
cargo make up             # headless; serial in the run log; Ctrl-C kills QEMU
cargo make start          # same, background
cargo make ssh            # ssh -p 2222 home@127.0.0.1
cargo make stop
```

Serial is a **root** shell. SSH is **home** (`sudo` has no password). Then:

```
oath
oath ls
```

See [using.md](using.md), [catalog.md](catalog.md), [services.md](services.md),
[generations.md](generations.md).

Headless (`up` / `start`) does not attach serial. They inject host
SSH public keys into the guest at boot. Then `cargo make ssh` is
`ssh -p 2222 home@127.0.0.1` (`OATH_SSH_PORT`). Extra args:
`cargo make ssh -- ls /`. Serial is `build/runs/<id>/serial.log`.

## Leave the VM

Ctrl-D (or `exit`) leaves the **shell**. `svc:serial` is `restart=always`,
so PID 1 starts it again. That is not leaving QEMU.

There is no QEMU monitor (`-monitor none`), so **Ctrl-A x** does not work.
Ctrl-C goes to the guest (`signal=off`). `cargo make up` is the opposite:
Ctrl-C kills QEMU. `cargo make stop` kills a `start`ed VM.

From the guest, halt (powers off the VM so QEMU exits):

```
oath set host:local power=halt
oath apply --confirm
```

`cargo make run` also exits if the guest reboots (`-no-reboot`).

If the guest is already halted and this prompt is dead, QEMU is still
holding the terminal. From **another** host terminal:

```
pkill -f qemu-system-x86_64
```

## Probe (scripted)

```sh
cargo make probe
```

Writes `build/runs/<id>/`: `meta.json`, `serial-boot*.log`, `events.jsonl`
(`oath-tel` lines), `probe.json`, `REPORT.md`. Uses a qcow overlay so
the golden image is not mutated.

`cargo make run` also writes a run dir (`serial.log`).

Host orchestration is `cargo make` (crate `oath-make`). QEMU, mkfs.btrfs,
and a loop-mount (sudo) are still external tools.

## What is in the image

- Linux kernel (borrowed) + initramfs (`oath-init` as `/init`)
- btrfs disk, live subvolume `@`
- `/lib/oath/init` — PID 1 after switch-root
- Catalog at `/oath`
- `/oath/store/pkg/{busybox,btrfs,oath,dropbear,glibc,river,hello}/`; `/bin` is a symlink farm
- `/bin/hello` only after `pkg:hello` is present

## Telemetry

Guest: JSON on stderr, prefixed `oath-tel `, and `/oath/log/*.jsonl`
once the disk is mounted.

## What works

- Boot to serial, `oath` verbs
- Hostname apply, undo, reboot (hostname survives)
- `power=reboot` without `--confirm` refuses (exit 3)
- Sibling generations `/oath/run/fs/@gen-N`
- `svc:hold` start / stop / undo / persist across reboot
- `pkg:hello` install / remove / undo / persist across reboot
- `pkg:grok` present; `/bin/grok --version`; store ELF root-owned
- `pkg:git` / `pkg:curl` present; `git --version`; `curl --version`
- `pkg:thoxa` present on next image (`/bin/thoxa` login shell; canto live this boot)
- `pkg:busybox` / `btrfs` / `oath` present and not removable
- `dev:vda` / `net0` / `ttyS0` inventory; tmpfs + cgroup2
- `net:net0` up / ping gateway / down / undo / reboot persist
- SSH: inject pubkey, login, empty keys deny, undo, reboot persist
- scp/sftp as `home` (`/bin/scp`, `/bin/sftp-server` in `pkg:dropbear`; next image)
- guest `/bin/ssh` (dropbear dbclient; canto live; QEMU next image)
- busybox `/bin/vi`
- `svc:seatd` + `svc:river` (pixman on virtio-gpu); Wayland socket under `/run/user/1`
- Sola session stack (`svc:sola-bus` / `call` / `river` / `shell` /
  `session`); `sola-terminal` + tmux + `sola-browser` + CEF +
  `sola-workspaces` + `solactl` + SF Pro Text / Iosevka Term Slab in
  `pkg:sola`; no nested process manager
- virtio keyboard + mouse (`dev:kbd0` / `dev:mouse0`; libinput via
  libudev-zero, no udevd)

## Limits

- virtio-gpu (`/dev/dri/card0`). gtk window when `DISPLAY` is set is
  pixman River plus the Sola menubar at **1280×800 1:1** (software GL,
  McMojave cursor; no udevd). Guest scanout matches the window:
  virtio-gpu `xres`/`yres`, gtk `zoom-to-fit=off`, `GDK_SCALE=1`, and
  `SOLA_OUTPUT_PICK=preferred` so sola-river does not promote a 4K
  CVT mode from virtio-gpu’s mode list. `OATH_DISPLAY_WIDTH` /
  `OATH_DISPLAY_HEIGHT` override (640–7680). `OATH_DISPLAY=none` hides
  it. Probe is headless.
- QEMU user net + virtio-net; `net:net0` is `10.0.2.15/24`
- SSH hostfwd `127.0.0.1:2222` → guest 22 (`OATH_SSH_PORT`). Optional
  `OATH_BRIDGE=br0` for a host bridge (no hostfwd).
- No installer
- No boot-generation picker (undo is the supported rewind)
