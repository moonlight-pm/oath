# Services

PID 1 is **oath-init** (`/lib/oath/init`, also `/sbin/init`). It is
not a catalog object. Every supervised process is a `svc:*` object.
There is no unit file, no systemd, no `/etc/init.d`.

## What ships

| Id | Exec | Default | Notes |
|----|------|---------|-------|
| `svc:serial` | `/lib/oath/serial-login` | enabled, `restart=always` | Root shell on serial (`ttyS0` / `hvc0`). It does not take the graphical VT; that stays on the boot mark. Do not disable it unless you have another console. |
| `svc:hold` | `/bin/sleep 86400000` | enabled, `restart=always` | Harmless sleeper for start/stop. |
| `svc:sshd` | dropbear | enabled, `restart=always` | Keys in `ssh:local`. Password off. SFTP via `/bin/sftp-server`; `/bin/scp` for legacy `scp -O`. |
| `svc:seatd` | `/bin/seatd -u home -g home` | enabled, `restart=always` | Seat for DRM. Socket owned by `home`. `svc:river` wants this. |
| `svc:river` | `/lib/oath/run-compositor` | enabled, `restart=always` | Wrapper around `/bin/river` (runs as `home`): picks the DRM card with a connected connector (dual-GPU). GLES2/radeonsi on real KMS, pixman on virtio. libinput via libudev-zero. |
| `svc:sola-bus` | `/bin/sola-bus` | enabled, `restart=on-failure` | Sola IPC bus. Socket `/run/user/1/sola-bus`. |
| `svc:sola-call` | `/bin/sola-call` | enabled, `restart=on-failure` | Sola call host. Socket `/run/user/1/sola-call`. |
| `svc:sola-river` | `/bin/sola-river` | enabled, `restart=on-failure` | Bridge (bus ↔ Wayland). Wants `svc:river` + bus + call. Not the compositor. |
| `svc:sola-shell` | `/bin/sola-shell` | enabled, `restart=on-failure` | Iced menubar / launcher / window menu / Super+K / Super+Tab. Wants river + bus + call + the bridge. wgpu/gl; llvmpipe only on virtio KMS; gles2/radeonsi on metal. |
| `svc:sola-session` | `/bin/sola-session` | enabled, `restart=on-failure` | LaunchApp / CloseApp owner. Wants bus + call. Direct spawn (no systemd). Kit apps in `pkg:sola`: `/bin/sola-terminal` (tmux, `$SHELL` default `/bin/thoxa`; first tab `$HOME`), `/bin/sola-browser` (CEF), `/bin/sola-workspaces` (`solactl` helper; same `$SHELL`). |
| `svc:sola-kvm` | `/bin/sola-kvm listen` | enabled, `restart=always` | Linux KVM **client** as `home`. UDP 4242 injects via River virtual pointer/keyboard (key before modifiers, so Super+Tab confirms on Super-up; kernel auto-repeat is not injected). CLIP1 TCP on the same port syncs text and `image/png` on Enter/Leave. Wants `svc:river`. |
| `svc:pipewire` | `/bin/pipewire` | enabled, `restart=always` | Seat audio graph as `home`. `/run/user/1/pipewire-0`. Wants nothing. |
| `svc:wireplumber` | `/bin/wireplumber` | enabled, `restart=always` | Session manager (`--profile main-embedded`). Wants `svc:pipewire`. |
| `svc:pipewire-pulse` | `/bin/pipewire-pulse` | enabled, `restart=always` | Pulse compatibility for librespot. Wants `svc:pipewire`. |
| `svc:backup` | `/lib/oath/backup-daily` + NFS spec | **off** in seed; **on** canto, `restart=always` | Sleeps until 04:00 US Mountain, then one `btrfs send` of `@` to NFS (overwrite `canto.send` + sidecar). Dest `10.0.0.12:/mnt/alpha/backup/canto`. Packs may ship `libexec/oath-backup-quiesce` / `thaw`. Manual send: `/lib/oath/backup-send`. |

## Audio

The menubar volume chip appears only when `pw-dump` works. Default sink
on canto is **Built-in Audio** (Intel HDA PCH analog). The 12-band LED
spectrum is a `pw-cat` tap on that sink. `wpctl` changes volume.

```
XDG_RUNTIME_DIR=/run/user/1 wpctl status
XDG_RUNTIME_DIR=/run/user/1 pw-dump | head
```

On canto **this boot**, PipeWire was started as `home` by hand
(`XDG_RUNTIME_DIR=/run/user/1`) so the chip has a sink before the next
ESP initrd (the running PID 1 does not treat `svc:pipewire` as a seat
svc). New images seed the three svcs as seat. A reboot without that
initrd drops the ALSA modules and the daemons.

No dbus-daemon: MPRIS / BlueZ stay quiet. HDMI heads exist as ALSA
cards but are not udev-enumerated.

**Quit Sola** (flower menu) broadcasts shutdown and the session processes
exit 0. PID 1 does **not** restart them (`on-failure` only). River the
compositor, serial, and SSH stay. Reboot or `oath apply` (while they
remain `enabled=true`) starts the session again. To leave it off across
reboot: `oath set svc:sola-shell enabled=false` (and bus, call, bridge,
session) then `oath apply`.

**Restart Computer** / **Shut Down** (flower menu) are `host:local`
`power=reboot` / `power=halt`. The click is the owner's confirm:
`sudo oath apply --confirm host:local`. There is no logind.

PID 1 starts enabled services at boot, **stops disabled ones** on
converge, reaps, and restarts per policy (`never` / `always` /
`on-failure`).

`oath apply` on a `svc` writes desired, notifies PID 1 on
`/oath/run/init.sock`, and waits until actual `state` is `running` or
`stopped`. `oath undo` notifies PID 1 after restoring the catalog.

`wants` is start order: enabled wanted services start first. Cycles
refuse apply. `svc:hold` wants `svc:serial`.

## Start / stop

```
oath ls --kind svc
oath get svc:hold
oath set svc:hold enabled=false
oath apply svc:hold
oath get svc:hold --actual
oath undo
```

Actual `state` is `stopped` | `starting` | `running` | `failed`.

## Backup (NFS)

One off-box copy of the live `@` subvolume. Not a second undo.
Schedule and dest live on `svc:backup`.

```
oath get svc:backup
cat /oath/objects/svc/backup/last.json
```

Canto dest is `10.0.0.12:/mnt/alpha/backup/canto` (NAS share `alpha`,
folder `backup/canto`). After a send:

| File | What |
|------|------|
| `canto.send` | `btrfs send` stream of that generation (overwritten) |
| `canto.json` | hostname, generation, **UTC** time (`…Z`), size, sha256 |

The sleeper is `/lib/oath/backup-daily`. It waits until **04:00 US
Mountain** (POSIX TZ inside that process only) then runs
`backup-send`. The **system clock stays UTC** — `date` on the box is
Zulu. Seed leaves `svc:backup` **off** so QEMU does not hit the NAS.
Canto has it **on**.

Send now (overwrites the one NAS copy; previous file kept until the
new send finishes):

```
sudo /lib/oath/backup-send 10.0.0.12:/mnt/alpha/backup/canto
```

Stop the daily sleeper:

```
oath set svc:backup enabled=false
oath apply svc:backup
```

Snapshot is crash-consistent (`sync` + btrfs CoW). Packs that need a
freeze (postgres WAL, etc.) may ship
`/oath/store/pkg/<name>/libexec/oath-backup-quiesce` and
`oath-backup-thaw`. No hook means skip. Reboot without a rebuilt ESP
initrd drops nfs.ko (canto this boot loaded it by hand).
