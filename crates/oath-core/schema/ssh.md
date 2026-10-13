# ssh

Owner SSH keys. One object: `ssh:local`. Dropbear is `svc:sshd`.
SSH login is **home**. There is no login password and no private key in
the image. Root SSH is denied. Serial stays root.

**When to use:** add or replace the public keys that may log in as
home.

**When not:** do not edit `/home/.ssh` as admin. Do not put a private
key in the catalog. Host keys live under `/oath/ssh/` and are
generated on first boot.

## Fields

- `authorized` — list of OpenSSH public key lines (`ssh-ed25519 …`).
  Apply writes `/home/.ssh/authorized_keys`.

Safety: `mutate`.

Known gap: replacing `authorized` with keys you do not hold locks you
out over SSH. Apply does not ask for `--confirm` or roll back.

## Example

```
oath get ssh:local
oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
oath apply
```

The installer writes the build host's public keys here. From the QEMU
host: `ssh -p 2222 home@127.0.0.1` (or `cargo make ssh`).
