# Network, SSH and the accounts

An Oath box is reached over SSH as the account `home`, with keys listed
in the catalog. There is one NIC object, `net:net0`, which takes a
DHCP lease by default. Root is privilege, not a login: `home` uses
`sudo`, the screen (tty1) is a `home` shell when no desk owns it, and
the serial console is the root break-glass.

**Status:** partly built. DHCP, static addresses, SSH keys, host keys,
the `home` account and `sudo` are built. Changes that can cut off SSH
need `--confirm`, and a supervised DHCP client renews the lease (both
built 2026-10-13). `sudo` was hardened on 2026-10-13. Those changes go
on probation and PID 1 rolls them back unless a new session keeps them
(built 2026-10-16, catalog.md).

Replaces: T9 (net), T15 (SSH and DHCP), the account parts of T31.

## `net:net0` (built)

```json
{ "up": true, "ipv4": "dhcp", "gateway": "", "lease": null }
```

- Converge finds the sole non-loopback interface and renames it `net0`.
  The agent never hunts for `eth0` or `ens3`.
- `ipv4` is `dhcp` or a CIDR. With a CIDR, `gateway` sets the default
  route.
- `dhcp`: converge runs `dhcpcd -1` once for the first lease (R18;
  dhcpcd configures the address and route, its output stays out of
  `oath apply`) and records `lease` in actual. `lease` is not a desired
  field. While the applied `net:net0` is up and `dhcp`, PID 1 also runs
  `dhcpcd -B … net0` as an internal service (id `net:net0`, not a
  catalog `svc`), restarted with backoff, so the lease renews. Both are
  IPv4 only (`-4`), keep the address when they stop (`-p`), take no
  link-local address (`-L`), identify by the hardware address
  (`-I ''`) and run `/lib/oath/dhcpcd.hook` in place of dhcpcd's own
  hooks (`-c`). dhcpcd separates privileges (R36): the processes that
  read the network run as `void`, chrooted in `/var/empty`, and its root
  process configures the address and runs the hook. Its seccomp filter
  is built off, since a syscall missing from its list would kill those
  processes and cut a headless machine off. Its log is
  `/oath/log/net/net0.log` (`oath logs net:net0`). Apply notifies PID 1
  after every `net` converge; a spec that wants no client (static, or
  down) is handed to PID 1 first, so the client is stopped before the
  address changes (dhcpcd would otherwise put its lease back).
- Every lease the client gets is recorded: the hook writes
  `/etc/resolv.conf` and runs `/lib/oath/net-lease` (PID 1's binary)
  with dhcpcd's environment (`reason`, `new_*`), which writes actual
  `lease` (the address) and `dhcp` (`address`, `router`, `dns`,
  `seconds`, `server`, `event`, `at`) without the catalog lock, and
  keeps a copy in `/run/oath/net0.lease.json` that the next converge
  reads. `BOUND` and `REBOOT` record a `bound` event, `RENEW` and
  `REBIND` a `renew`; `EXPIRE`, `NAK`, `RELEASE`, `NOCARRIER` and
  `DEPARTED` clear both.
- Any change to `up`, `ipv4` or `gateway` is confirm-class: `oath apply`
  exits 3 without `--confirm`, because it can cut the machine off from
  SSH. Serial still works.
- PID 1 brings `lo` up itself. Loopback is not a catalog object.
- The base profile seeds DHCP. QEMU user networking hands out
  10.0.2.15 and forwards host port 2222 (`OATH_SSH_PORT`) to guest 22.

## `ssh:local` (built)

```json
{ "authorized": ["ssh-ed25519 AAAA… owner@laptop"] }
```

- `authorized` is a list of OpenSSH public key lines (each starts
  `ssh-`, `ecdsa-` or `sk-`; `set` refuses anything else). Converge
  writes them to `/home/.ssh/authorized_keys`, owned by `home`, mode
  600. At boot PID 1 applies the keys last applied (`applied.json`),
  not pending desired.
- `oath undo` never restores `/oath/ssh/`: host keys do not roll back.
- The host key is `/oath/ssh/ssh_host_ed25519_key` (and `.pub`),
  made on first converge with `ssh-keygen`, or by the installer, which
  pins it. No image ships a private key. A machine from before R36 has
  dropbear's key at `/oath/ssh/host_ed25519`: converge writes the same
  key in OpenSSH's format once, so the pins in known_hosts still hold.
- `svc:sshd` is OpenSSH's sshd (R36), `/bin/sshd -D -e`, with
  `keep_children`: restarting sshd signals only the listener, so open
  login sessions survive (and `oath apply` waits for PID 1 to have
  restarted it). `pkg:openssh` (OpenSSH 10.6p1, with `pkg:openssl` for
  RSA and ECDSA keys: owners' keys are often RSA) is fixed (not removable): `sshd`, with `sshd-session`
  and `sshd-auth` in its `libexec/`, `ssh`, `scp`, `sftp`, `ssh-keygen`
  and `sftp-server`; it links `crypt(3)` from `pkg:libxcrypt`. Its
  unauthenticated process runs as `void`, chrooted in `/var/empty`.
- `/etc/ssh/sshd_config` is written with `/etc/profile` (one writer,
  from `host:local`): the host key, keys only (no passwords, no
  keyboard-interactive), `PermitRootLogin no`, no pid file, SFTP by
  `/bin/sftp-server`, and one `SetEnv` line with `host:local` env and
  `TZ`. So `ssh host 'command'`, which reads no `/etc/profile`, has
  `SSL_CERT_FILE`, `TZ` and `LANG`; sshd sets `USER`, `HOME`, `SHELL`
  and `PATH` itself. sshd reads the file when it starts, so a changed
  env reaches SSH commands after sshd restarts (a login shell reads
  `/etc/profile` each time).
- `cargo make update` moves a machine from dropbear: once it has booted
  a PID 1 that wrote the config and converted the key, `svc:sshd` is
  set to OpenSSH's sshd and applied on probation; a new login keeps
  it. `pkg:dropbear` stays on such a machine, unused, until its owner
  removes it.
- The installer writes the build host's public keys into `ssh:local`
  through the install overlay (boot-and-install.md).

## Time (built)

- `svc:time` in the `base` seed runs `/bin/chronyd -d 'server
  0.pool.ntp.org iburst' … 'makestep 1 3'` (R12, R18): its
  directives are arguments, there is no config file, and it is built
  without a command socket (no `chronyc`) or NTS. It starts as root and
  drops to `void` (R36, built `--with-user=void`), keeping only
  `CAP_SYS_TIME` and `CAP_NET_BIND_SERVICE` through libcap. It steps an offset
  over one second in its first three updates and slews after that; its
  own log lines are the service log.
- The RTC is not written; every boot steps the clock again.

## Accounts (built)

- `root` (uid 0, shell `/bin/sh`), `home` (uid 1, gid 1,
  `HOME=/home`, shell `/bin/zsh`, R27) and `void` (R36: uid and gid
  65534, home `/var/empty`, shell `/bin/false`, which `/etc/shells`
  does not list). There are no others and no user kind. `void` owns no
  files and has no keys; sshd, dhcpcd and chronyd drop to it, and a
  service may run as it. PID 1 makes `/var/empty` (root's, 0755) at
  boot. `/etc/passwd`, `/etc/group`, `/etc/shadow` and
  `/etc/shells` are written at pack time; `/etc/hostname`,
  `/etc/hosts`, `/etc/profile`, `/etc/zprofile` and `/etc/zshrc` are
  side effects of converging `host:local` (and of PID 1 reading it at
  boot), through one writer (`seat::write_side_effects`).
- zsh (`pkg:zsh`, fixed) is the login shell. `/etc/zprofile` reads
  `/etc/profile` in sh emulation, so a login zsh has `host:local`
  env. `/etc/zshrc` is the prompt Joshua's NixOS machine uses (time,
  `%n@%m`, `%2~`, the git branch, `§`) with `user@host` and the branch
  in a hue from the FNV-1a hash of the hostname (at #3B9DC2's and
  #2A7A9E's saturation and lightness), history in `~/.zsh_history`,
  emacs keys with Home, End and Delete, and `compinit`. The zsh
  package has no `newuser` script, so a first shell asks nothing.
  `/etc/hosts` is localhost, the hostname, then `host:local.hosts`
  (built 2026-10-19): host name to one IPv4 or IPv6 address, for LAN
  machines the network's DNS does not serve. Names follow the
  hostname's characters (at most 253); `localhost`, the machine's own
  hostname and an address that does not parse are refused at `set`.
- `home` runs `sudo <cmd>` with no password. `/bin/sudo` is a setuid
  helper at `/lib/oath/sudo`, not PAM and not a package. Applying is
  `sudo oath apply`. `sudo [-n] [--] command [args…]` refuses any
  other option, sets groups, gid and uid to root and checks each call,
  and runs the command with a clean root environment (`PATH=/bin`,
  `HOME=/root`), keeping only `TERM` and `TZ`.
- The serial console logs in as root; the screen (`svc:screen`, tty1)
  as `home`, so `/dev/tty1` is home's (PID 1 sets it 0600 after every
  discovery pass). The installer ramdisk's sshd accepts
  root over SSH with the baked host keys; the installed system does not.

## Known defects (from the 2026-10-13 review)

- None open. The two from the review were fixed on 2026-10-16: renewals
  are recorded in actual, and lockout-class changes (`svc:sshd`
  disabled, removed, or its `exec`, `user` or `env` changed;
  `ssh:local` losing a key; any `net:net0` `up`, `ipv4` or `gateway`
  change) go on probation and roll back unless a new session keeps
  them.

## Target

- **More NICs.** Naming beyond `net0` waits until a machine needs it.
- **Root SSH as a field.** A payload that needs root over SSH gets a
  generic `ssh:local` toggle, off by default (scope.md). Not before a
  payload needs it.

## Courage test

From the build host: `ssh -p 2222 home@127.0.0.1 oath get net:net0`
shows a lease, and `ps` shows `dhcpcd: net0 [ip4]`. Adding a second key to `ssh:local` and applying lets
that key log in. `sudo oath apply` works; `ssh root@…` is refused.
