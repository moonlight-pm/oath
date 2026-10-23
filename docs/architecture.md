# Architecture (as built)

What runs and where things live today. Target design is in
[specs/](specs/README.md). Update this file when a process, path or
tree changes.

**As of:** 2026-10-17, base profile, booted in QEMU and on canto.

## Boot

```
firmware ── ESP: EFI/BOOT/BOOTX64.EFI = oath-efi (menu, mark; the only loader)
            loader/entries/oath.conf  + oath-N.conf (last 5 boots)
            vmlinuz (own build, 7.3.0-rc1) + initrd.gz
  kernel cmdline: oath.root=<dev> oath.subvol=@ console=ttyS0 panic=10 …
  initrd /init = oath-init (PID 1, static against glibc); bash (and its
    libraries and glibc's loader in /lib) for the fallback shell
    catch SIGTERM/SIGINT (reboot), SIGUSR1 (halt), SIGUSR2 (poweroff)
    open the uevent socket (netlink) before the first coldplug
    mount /proc /sys /dev /dev/pts; tmpfs /tmp /run /dev/shm; cgroup2
    kernel modprobe helper = /lib/oath/modprobe (this binary)
    coldplug by modalias (display and sound wait until services start;
      the screen keeps the EFI mark until then, then shows tty1, svc:screen)
    mount <root> subvol=@ at /newroot, bind initrd modules+firmware
    switch_root: move /dev /proc /sys /run into it, move it over /, chroot
    mount btrfs subvolid=0 at /oath/run/fs (generations are siblings of @)
    tmpfs at /oath/status (status; no snapshot holds it)
    seed only if /oath/objects is missing; regenerate a missing INDEX.md;
      migrate a catalog from before R14 (old actual.json → applied.json)
    under /oath/.lock: apply host, net (one-shot dhcpcd for dhcp), ssh
      from their applied specs (applied.json); discover dev:* from sysfs;
      observe pkg:*, plan:* and snap:current into status/
    listen on /oath/run/init.sock; start svc:* from applied.json in
      `wants` order (a cycle skips only its members) and the DHCP client;
      after `ready`, run `oath verify` once (re-hash the linked store)
    main loop: shutdown request? reap, pump logs, start due restarts,
      the probation timer (runs `oath undo --expired` once it is up),
      uevents (load the module an added device asks for; discover
      dev:* again after a block/net/tty change), answer converge requests;
      a panic in a pass writes /run/oath/init-resume.json and execs
      /proc/self/exe oath-resume, which takes the state and fds back
```

QEMU (`cargo make run|up|start`) boots the same kernel and initrd with
`-kernel` against `build/out/latest/oath.qcow2` (through an overlay in
`build/runs/`) and forwards SSH to 127.0.0.1:2222.

## Disk

```
GPT  1: ESP (vfat, 512M)     2: btrfs "oath"
btrfs
  @           live root
  @gen-N      read-only snapshot taken by each apply; undo copies the
              touched objects' spec back out of @gen-N/oath/objects
  @boot-N     snapshot taken when `cargo make esp` archives a boot
```

## Root tree

```
/sbin/init → /lib/oath/init        same ELF as the initrd's /init
/lib/oath/  serial-login, screen-login, sudo (setuid), dhcpcd.hook, backup-send, backup-daily;
            modprobe and net-lease are links to init (the kernel's module
            helper, and the DHCP script's lease recorder)
/bin/       symlink farm → /oath/store/pkg/<name>/<hash>/bin/<x>; modprobe, reboot,
            halt and poweroff → /lib/oath/init
/lib/       symlink farm → /oath/store/pkg/<name>/<hash>/lib/<x> (R17): glibc's
            loader ld-linux-x86-64.so.2 and every linked package's libraries;
            no /lib64. oath/, modules/ and firmware/ are not links
/home       the `home` account (uid 1); SSH logs in here
/etc        written by PID 1 from host:local (hostname, hosts, profile, shells)
/oath/      the catalog
  INDEX.md                 generated; what an agent reads first
  schema/<kind>.json|.md   per-kind schema (generated from the type) and
                           prose; PID 1 rewrites them at boot if they differ
  objects/<kind>/<name>/   desired.json applied.json meta.json
                           [new | remove markers for oath new / oath rm]
  status/<kind>/<name>.json  actual: tmpfs mounted by PID 1, rebuilt each boot
  log/apply.jsonl          one line per apply (ok, error, refused), undo or keep
  probation.json           the apply on probation (lockout class), while open
  log/init.log             PID 1's messages (`oath logs`)
  log/svc/<name>.log       each service's stdout+stderr, UTC-stamped,
                           rotated to .1 past 256 KiB (`oath logs svc:<name>`)
  log/net/net0.log         the DHCP client (`oath logs net:net0`)
  log/plan/<name>.log      the last `oath build` of a plan, .1 the one
                           before (`oath logs plan:<name>`)
  .lock                    exclusive flock held by catalog writers
  .gens/<n>/objects/       copy snapshots (off btrfs or under --root only)
  store/pkg/<name>/<hash>/ package trees; `live` → the linked hash
  store/plan/<name>/<hash>/plan.plan   plans; the image files each recipe's (R23)
  store/adv/<name>/<hash>.json   advisories
  ssh/host_ed25519         dropbear host key
  run/                     init.sock, fs/ (btrfs top level)
```

## Processes (base profile)

| Process | From | Runs as |
|---------|------|---------|
| `oath-init` | initrd `/init` | PID 1, root |
| `svc:sshd` | `/bin/dropbear -F -E -s -w` | root (logins are `home`; root refused); `keep_children` |
| `svc:serial` | `/lib/oath/serial-login` on ttyS0 (idles with no UART) | root (break-glass) |
| `svc:screen` | `/lib/oath/screen-login` on tty1, then `zsh -l` | `home` |
| `svc:time` | `/bin/chronyd -d` against `0`–`3.pool.ntp.org` | root |
| DHCP client | `dhcpcd -B … net0` with `/lib/oath/dhcpcd.hook`, supervised by PID 1 as `net:net0` | root; renews the lease |
| `oath mcp` | one per SSH connection that asks for it (`ssh -T home@<host> oath mcp`); not supervised, no port | `home`; each write tool runs `sudo -n oath --json …` |

The `desk` profile adds (R20–R22; `svc:screen` is off):

| Process | From | Runs as |
|---------|------|---------|
| `svc:seatd` | `/bin/seatd -u home -g home`; opens DRM and input devices for the seat | root |
| `svc:rivulet` | `/bin/rivulet -c :`, the compositor (wlroots, DRM, libinput, Xwayland on demand) | `home` |
| `svc:sola-bus`, `svc:sola-call` | Sola's message bus and call plane (`/run/user/1/sola-*`) | `home` |
| `svc:sola-river` | Sola's window manager, a client of rivulet's River protocols | `home` |
| `svc:sola-shell` | the menubar, launcher and switcher | `home` |
| `svc:sola-session` | launches apps (`sola-terminal`, `sola-projects`, `sola-browser`, `sola-design`) and reopens the ones open before; their logs are `/home/.local/state/sola/app-*.log` | `home` |
| `svc:sola-tmux`, `svc:sola-tmux-ws` | the tmux servers (sockets `sola`, `sola-ws`) that hold the terminal's and Projects' shells, outside the session's cgroup so they outlive its restarts | `home` |
| `svc:sola-kvm` | `sola-kvm listen --bind 0.0.0.0:4242`: novus's `sola-kvm server` sends it pointer and keys when its cursor crosses the edge | `home` |
| `svc:dbus-session` | `/bin/dbus-daemon --session` on `/run/user/1/bus` (`DBUS_SESSION_BUS_ADDRESS` in `host:local` env) | `home` |
| `svc:pipewire`, `svc:wireplumber`, `svc:pipewire-pulse` | PipeWire, its session manager and its PulseAudio server (`/run/user/1/pulse/native`, which CEF's libpulse finds) | `home` |

Every supervised process has its own cgroup,
`/sys/fs/cgroup/oath/<kind>-<name>`, and writes stdout+stderr into a
pipe PID 1 drains into its log. Stopping a service signals the whole
cgroup (only the main process with `keep_children`), waits up to 5 s,
then `cgroup.kill`s the rest.

## Shutdown

`reboot`, `halt`, `poweroff`, Ctrl-Alt-Del and `oath` `power=` all
signal PID 1. It stops the services in reverse `wants` order, a layer
at a time (TERM, 5 s, KILL), signals
everything else, syncs, lazily unmounts `/oath/run/fs`, remounts `/`
read-only, then calls `reboot(2)`.

## Host side (novus)

```
cargo make bootstrap
  stage 0 (host gcc, nix-shell pkgs/bootstrap.nix): a cross binutils, GCC
    and glibc for x86_64-oath-linux-gnu, under build/bootstrap/, never ships
  stage 1 (by stage 0): pkg:gcc, pkg:glibc and the GNU base tools (bash,
    coreutils, sed, grep, gawk, findutils, diffutils, patch, tar, gzip, xz,
    bzip2, make) into the build catalog
cargo make pkgs [names]
  sources → build/fetch (sha256 pin; upstream signature, cached under
    build/fetch/sig/, checked against pkgs/keys/<key>.asc)
  pkgs/<name>/pkg.toml + build.sh → source pack pkg:<name>-src
    + pkgs/<name>/<name>.plan (build packs pinned from pkgs/lock.json)
  `oath build` in the sandbox of the build catalog at ~/.local/share/oath
  product hash, version, licence, plan hash → pkgs/lock.json
  the base packs rebuild themselves; python, meson and ninja are
    build-only packs, and so are m4, flex, bison, bc, perl and elfutils
    (libelf), which the kernel's plan needs; each build also gets what
    its build packs run with (the lock records it)
  pkg:rust: the vendor's rustc, cargo and std (glibc, uefi); patchelf
    points its binaries at /lib's loader
  a recipe (R23, pkg:claude-code): built here only for its product hash;
    the image files its plan and pins pkg:<name>-src (the vendor's url
    and file) and pkg:<name>, both absent; the machine builds it
  pkg:linux: boot/vmlinuz, boot/config, lib/modules/<release>/
cargo make build [--profile base]
  pkg:oath: this tree + every crates.io crate in Cargo.lock → pkg:oath-src;
    `cargo build` with pkg:rust in the sandbox, static against glibc,
    linked by pkg:gcc → bin/oath,
    lib/oath/{init,serial-login,screen-login,sudo}, boot/BOOTX64.EFI (oath-efi)
  kernel + modules from pkg:linux (pkgs/linux, a plan), vermagic checked
  every userspace pack (glibc, bash, coreutils, btrfs, dropbear, …)
    from pkgs/lock.json; QEMU + OVMF from image/host.nix (host only)
  ~/.local/share/oath/store/
                          host copy of every packed tree, by hash
                          (OATH_STORE; outside the checkout)
  build/out/<id>/         one build output, written once (<id>: the
                          manifest's hash; an unchanged tree reuses it;
                          build/out/latest names the newest; five kept)
    manifest.json         every shipped pack: hash, licence, plan, where
                          it ships, source pins and keys
    stage/                the root tree; `oath seed --profile` writes /oath
    oath.qcow2            btrfs @ with the stage
    bzImage, initrd.gz, BOOTX64.EFI
    initrd-install.gz     installer ramdisk: dropbear, sfdisk, wipefs,
                          mkfs.fat, mkfs.btrfs, btrfs, mount, ip, dhcpcd,
                          tar, coreutils, oath, oath-efi, each with the
                          libraries it loads (install bakes keys into a
                          copy)
    check-pass            a clean `cargo make check` of this output
cargo make publish [--dest <dir>]
  every locked pack and every profile-named one, with its plan, source
  pack and advisories, then INDEX.md (R25): without --dest, curl
  --aws-sigv4 lists the R2 bucket oath-store and puts what it lacks,
  key from ~/.config/oath/origin.env; machines read it at
  https://store.oath.moonlight.pm (host:local.origins)
cargo make check
  one QEMU per case in check/cases.toml (4 at a time), SSH as home with
  build/check/key; serves the fetch canary and a one-pack origin
  (/origin/INDEX.md, /origin/pkg/fetchme/…) on 127.0.0.1:18765; cases
  make their own fixtures (`oath new`); a clean run writes
  build/out/<id>/check-pass (kernel, initrd, image sha256)
cargo make check --target home@<host>
  the `metal` cases on an installed machine, one at a time, host key
  from build/known_hosts; undo back after each case; logs in
  build/check/<time>-<host>/
cargo make check --agent [--model m] [--target home@<host>]
  each task in check/agent.toml: a fresh QEMU (ports 2400+), setup over
  SSH, then the client (claude -p, headless, ANTHROPIC_API_KEY from the
  environment) with one MCP server, `ssh -T … oath mcp`; expect steps
  and the apply log judge it; build/out/<id>/agent-check.json and
  agent/<task>.jsonl transcripts; disks, serial and steps logs in
  build/check/<time>-agent/
cargo make install --target … --disk … --overlay hosts/<name>.json --confirm
  takes --output <id> or the newest checked output; refuses one
  without a matching check-pass; bakes keys into a copy of its
  installer ramdisk in build/runs/<time>-install/
  kexec (native pkg:kexec, copied to the target) or EFI-boot the
  installer, partition (sfdisk, wipefs), format, copy the stage, write
  the overlay with `oath seed --overlay`, make the host key and pin it
  in build/known_hosts, write the ESP, reboot; refuses the build host
cargo make update --target home@<host> --confirm [--reboot]
  over SSH with the pinned key: snapshot @ as @boot-<id>, tar the
  missing store packs into /oath/store, `oath set` the output's pins
  and one `oath apply`, replace /lib/oath and the static /etc files,
  rotate the ESP (archive the old boot), reboot and check PID 1;
  `--qemu <older output>` rehearses it in build/runs/<time>-install/
```
