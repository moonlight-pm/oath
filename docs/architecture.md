# Architecture (as built)

What runs and where things live today. Target design is in
[specs/](specs/README.md). Update this file when a process, path or
tree changes.

**As of:** 2026-10-15, base profile, booted in QEMU and on canto.

## Boot

```
firmware ── ESP: EFI/BOOT/BOOTX64.EFI = oath-efi (menu, mark; the only loader)
            loader/entries/oath.conf  + oath-N.conf (last 5 boots)
            vmlinuz (own build, 7.3.0-rc1) + initrd.gz
  kernel cmdline: oath.root=<dev> oath.subvol=@ console=ttyS0 panic=10 …
  initrd /init = oath-init (PID 1, static musl)
    catch SIGTERM/SIGINT (reboot), SIGUSR1 (halt), SIGUSR2 (poweroff)
    mount /proc /sys /dev /dev/pts; tmpfs /tmp /run /dev/shm; cgroup2
    kernel modprobe helper = /lib/oath/modprobe (this binary)
    coldplug by modalias (display and sound wait until services start)
    mount <root> subvol=@ at /newroot, bind initrd modules+firmware
    switch_root: move /dev /proc /sys /run into it, move it over /, chroot
    mount btrfs subvolid=0 at /oath/run/fs (generations are siblings of @)
    seed only if /oath/objects is missing; regenerate a missing INDEX.md
    under /oath/.lock: apply host, net (one-shot udhcpc for dhcp), ssh
      from their applied state (actual.json); probe dev
    listen on /oath/run/init.sock; start svc:* from applied.json in
      `wants` order (a cycle skips only its members) and the DHCP client
    main loop: shutdown request? reap, pump logs, start due restarts,
      answer converge requests
```

QEMU (`cargo make run|up|start`) boots the same kernel and initrd with
`-kernel` against `build/oath.qcow2` and forwards SSH to 127.0.0.1:2222.

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
/lib/oath/  serial-login, sudo (setuid), udhcpc.script, backup-send, backup-daily
/bin/       symlink farm → /oath/store/pkg/<name>/live/bin/<x>
/home       the `home` account (uid 1); SSH logs in here
/etc        written by PID 1 from host:local (hostname, hosts, profile, shells)
/oath/      the catalog
  INDEX.md                 generated; what an agent reads first
  schema/<kind>.json|.md   per-kind schema and prose
  objects/<kind>/<name>/   desired.json actual.json meta.json [applied.json]
                           [new | remove markers for oath new / oath rm]
  log/apply.jsonl          one line per apply (ok, error, refused) or undo
  log/init.log             PID 1's messages (`oath logs`)
  log/svc/<name>.log       each service's stdout+stderr, UTC-stamped,
                           rotated to .1 past 256 KiB (`oath logs svc:<name>`)
  log/net/net0.log         the DHCP client (`oath logs net:net0`)
  .lock                    exclusive flock held by catalog writers
  .gens/<n>/objects/       copy snapshots (off btrfs or under --root only)
  store/pkg/<name>/<hash>/ package trees; `live` → the linked hash
  store/plan/<name>/<hash>/plan.plan
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
| `svc:time` | `/bin/ntpd -n` against `0`–`3.pool.ntp.org` | root |
| DHCP client | `udhcpc -f -i net0`, supervised by PID 1 as `net:net0` | root; renews the lease |

Every supervised process has its own cgroup,
`/sys/fs/cgroup/oath/<kind>-<name>`, and writes stdout+stderr into a
pipe PID 1 drains into its log. Stopping a service signals the whole
cgroup (only the main process with `keep_children`), waits up to 5 s,
then `cgroup.kill`s the rest.

## Shutdown

`reboot`, `halt`, `poweroff`, Ctrl-Alt-Del and `oath` `power=` all
signal PID 1. It stops every service (TERM, 5 s, KILL), signals
everything else, syncs, lazily unmounts `/oath/run/fs`, remounts `/`
read-only, then calls `reboot(2)`.

## Host side (novus)

```
cargo make bootstrap
  stage 0 (host gcc, nix-shell pkgs/bootstrap.nix): clang/lld + musl sysroot,
    under build/bootstrap/, never ships
  stage 1 (by stage 0): pkg:llvm, pkg:busybox, pkg:make into the build catalog
cargo make pkgs [names]
  sources → build/fetch (sha256 pin; upstream signature, cached under
    build/fetch/sig/, checked against pkgs/keys/<key>.asc)
  pkgs/<name>/pkg.toml + build.sh → source pack pkg:<name>-src
    + pkgs/<name>/<name>.plan (build packs pinned from pkgs/lock.json)
  `oath build` in the sandbox of the build catalog at .cache/oath
  product hash, version, licence, plan hash → pkgs/lock.json
  llvm, busybox and make rebuild themselves; cmake, ninja and python
    are build-only packs (llvm's plan needs them), and so are m4, flex,
    bison, perl and elfutils (libelf), which the kernel's plan needs
  pkg:rust: the vendor's rustc, cargo and std (musl, uefi) with its own
    musl loader and libunwind as libgcc_s (patchelf points it there)
  pkg:linux: boot/vmlinuz, boot/config, lib/modules/<release>/
cargo make build [--profile base]
  pkg:oath: this tree + every crates.io crate in Cargo.lock → pkg:oath-src;
    `cargo build` with pkg:rust in the sandbox, linked by pkg:llvm's
    clang against its musl → bin/oath,
    lib/oath/{init,serial-login,sudo}, boot/BOOTX64.EFI (oath-efi)
  kernel + modules from pkg:linux (pkgs/linux, a plan), vermagic checked
  every userspace pack (busybox, btrfs, dropbear, openssh, firmware, …)
    from pkgs/lock.json; QEMU + OVMF from image/host.nix (host only)
  build/stage/            the root tree; `oath seed --profile` writes /oath
  .cache/oath/store/      host copy of every packed tree, by hash
  build/oath.qcow2        btrfs @ with the stage
  build/initrd.gz, build/initrd-install.gz (installer ramdisk: dropbear,
                          sfdisk, wipefs, mkfs.fat, mkfs.btrfs, btrfs,
                          busybox tar, oath, oath-efi)
  build/manifest.json     every shipped pack: hash, licence, plan, where
                          it ships, source pins and keys
cargo make check
  one QEMU per case in check/cases.toml (4 at a time), SSH as home with
  build/check/key; serves the fetch canary on 127.0.0.1:18765; cases
  make their own fixtures (`oath new`); a clean run
  writes build/check-pass (kernel, initrd, image sha256)
cargo make check --target home@<host>
  the `metal` cases on an installed machine, one at a time, host key
  from build/known_hosts; undo back after each case; logs in
  build/check/<time>-<host>/
cargo make install --target … --disk … --overlay hosts/<name>.json --confirm
  refuses an image without a matching build/check-pass
  kexec (native pkg:kexec, copied to the target) or EFI-boot the
  installer, partition (sfdisk, wipefs), format, copy the stage, write
  the overlay with `oath seed --overlay`, make the host key and pin it
  in build/known_hosts, write the ESP, reboot
```
