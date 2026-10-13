# Network, SSH and the accounts

An Oath box is reached over SSH as the account `home`, with keys listed
in the catalog. There is one NIC object, `net:net0`, which takes a
DHCP lease by default. Root is privilege, not a login: `home` uses
`sudo`, and the serial console is the root break-glass.

**Status:** partly built. DHCP, static addresses, SSH keys, host keys,
the `home` account and `sudo` are built. Changes that can cut off SSH
need `--confirm`, and a supervised DHCP client renews the lease (both
built 2026-10-13). Probation with automatic rollback and a stricter
`sudo` are the target.

Replaces: T9 (net), T15 (SSH and DHCP), the account parts of T31.

## `net:net0` (built)

```json
{ "up": true, "ipv4": "dhcp", "gateway": "", "lease": null }
```

- Converge finds the sole non-loopback interface and renames it `net0`.
  The agent never hunts for `eth0` or `ens3`.
- `ipv4` is `dhcp` or a CIDR. With a CIDR, `gateway` sets the default
  route.
- `dhcp`: converge runs busybox `udhcpc` once for the first lease
  (`/lib/oath/udhcpc.script` applies it; its output stays out of
  `oath apply`) and records `lease` in actual. `lease` is not a desired
  field. While the applied `net:net0` is up and `dhcp`, PID 1 also runs
  `udhcpc -f -i net0` as an internal service (id `net:net0`, not a
  catalog `svc`), restarted with backoff, so the lease renews. Its log is
  `/oath/log/net/net0.log` (`oath logs net:net0`). A static address
  stops it.
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
  600. At boot PID 1 applies the keys last applied (`actual`), not
  pending desired.
- `oath undo` never restores `/oath/ssh/`: host keys do not roll back.
- The host key is generated on first converge at
  `/oath/ssh/host_ed25519` with `dropbearkey`. No image ships a private
  key.
- `svc:sshd` runs dropbear in the foreground with `-s` (no passwords)
  and `-w` (no root login), with `keep_children`: restarting sshd
  signals only the listener, so open login sessions survive (and `oath
  apply` waits for PID 1 to have restarted it). `pkg:dropbear` is fixed (not removable).
  The package also carries the OpenSSH `ssh`, `ssh-keygen` and
  `sftp-server` binaries.
- The installer writes the build host's public keys into `ssh:local`
  through the install overlay (boot-and-install.md).

## Accounts (built)

- `root` (uid 0) and `home` (uid 1, gid 1, `HOME=/home`, shell
  `/bin/sh`). There are no others and no user kind. `/etc/passwd`,
  `/etc/group`, `/etc/shadow` and `/etc/shells` are written at pack
  time; `/etc/hostname`, `/etc/hosts` and `/etc/profile` are side
  effects of converging `host:local`.
- `home` runs `sudo <cmd>` with no password. `/bin/sudo` is a setuid
  helper at `/lib/oath/sudo`, not PAM and not a package. Applying is
  `sudo oath apply`.
- The serial console logs in as root. The installer ramdisk accepts
  root over SSH with the baked host keys; the installed system does not.

## Known defects (from the 2026-10-13 review)

- `sudo` drops every leading `-` argument, ignores the return values of
  `setgid` and `setuid`, never calls `setgroups`, and passes the
  caller's environment through.
- `actual.lease` is the first lease; renewals by the DHCP client are
  not written back.
- Changing `ssh:local`, `svc:sshd` or `net:net0` with `--confirm` can
  still lock the owner out; nothing rolls it back automatically. Without
  `--confirm` apply refuses: `svc:sshd` disabled, removed, or its
  `exec`, `user` or `env` changed; `ssh:local` `authorized` becoming
  empty; any `net:net0` `up`, `ipv4` or `gateway` change.

## Target

- **Probation.** Lockout-class changes apply on a timer and roll back
  unless confirmed from a fresh SSH session (catalog.md).
- **Current lease in actual.** The DHCP client's script records each
  renewal in `net:net0` actual.
- **`sudo`.** Parse only `--`; check every credential call; set
  supplementary groups to root's; clear the environment except a
  short allowlist (`TERM`, `LANG`, `TZ`), and set `PATH=/bin`.
- **More NICs.** Naming beyond `net0` waits until a machine needs it.
- **Root SSH as a field.** A payload that needs root over SSH gets a
  generic `ssh:local` toggle, off by default (scope.md).

## Courage test

From the build host: `ssh -p 2222 home@127.0.0.1 oath get net:net0`
shows a lease, and `ps` shows `udhcpc -f`. Adding a second key to `ssh:local` and applying lets
that key log in. `sudo oath apply` works; `ssh root@…` is refused.
