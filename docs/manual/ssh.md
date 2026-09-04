# SSH

SSH login is **home** (uid 1, `HOME=/home`). Root SSH is denied.
`sudo` has no password. Serial is root when that svc is enabled
(canto has no UART, so `svc:serial` is off). There is **no password**
and **no private key in the image**.

Host keys are generated on first boot under `/oath/ssh/`. Login keys
are `ssh:local` `authorized` (OpenSSH public key lines). Apply writes
`/home/.ssh/authorized_keys`.

`svc:sshd` is dropbear (`pkg:dropbear`, not removable). The same
package ships `/bin/sftp-server` (OpenSSH helper, musl static) and
`/bin/scp` (dropbear). Host `scp` / `sftp` as `home` work (canto
live; QEMU on the next pack). The editor is busybox `vi` (`/bin/vi`).
The `home` login shell is `/bin/thoxa` (`pkg:thoxa`; `thoxa -c` for
`ssh host cmd`). sola-terminal and sola-workspaces tmux panes use the
same `$SHELL` (wrappers default `/bin/thoxa`; they must not force
`/bin/sh`). Dropbear **rejects the user** unless that path is
listed in `/etc/shells` (the client error looks like publickey
denied). Root/serial stay `/bin/sh`.

## Add a key

```
oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
oath apply
```

`cargo make up` / `start` injects host public keys into `ssh:local`
(from `~/.ssh/*.pub`, default identities like `id_rsa` even without a
`.pub` file, `ssh-add -L`, or `OATH_SSH_PUBKEY`). They are not baked
into the image.

```
cargo make up          # another terminal:
cargo make ssh
cargo make ssh -- -i ~/.ssh/id_ed25519
# guest user is home; sudo has no password
```

Port: `OATH_SSH_PORT` (default 2222), forwarded to guest 22.

```
scp ./file home@canto:/home/file
sftp home@canto
```

QEMU: `scp -P "$OATH_SSH_PORT" ./file home@127.0.0.1:/home/file`.

Empty `authorized` means nobody can log in. Undo restores the last
keys. Host keys persist across undo of key lists (same machine).

A network install should **inject owner public keys** into `ssh:local`
and let the target generate host keys. Do not bake a login private key
into the image.
