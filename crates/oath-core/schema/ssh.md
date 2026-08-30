# ssh

Owner SSH keys. One object: `ssh:local`. Dropbear is `svc:sshd`.
Root is the owner. There is no login password and no private key in
the image.

**When to use:** add or replace the public keys that may log in as
root.

**When not:** do not edit `/root/.ssh` as admin. Do not put a private
key in the catalog. Host keys live under `/oath/ssh/` and are
generated on first apply. Serial is still how you reach a box with
empty `authorized`.

## Fields

- `authorized` — list of OpenSSH public key lines (`ssh-ed25519 …`).

Safety: `mutate`.

## Example

```
oath get ssh:local
oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
oath apply
```

From the QEMU host (user net): `ssh -p 2222 -i <key> root@127.0.0.1`
