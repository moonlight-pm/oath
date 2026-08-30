# SSH

Root SSH is the owner over the network. Serial is still the console.
There is **no password** and **no private key in the image**.

Host keys are generated on first boot under `/oath/ssh/`. Login keys
are `ssh:local` `authorized` (OpenSSH public key lines). Apply writes
`/root/.ssh/authorized_keys`.

`svc:sshd` is dropbear (`pkg:dropbear`, not removable).

## Add a key

```
oath set ssh:local --from-json '{"authorized":["ssh-ed25519 AAAA…"]}'
oath apply
```

QEMU user net (default):

```
cargo make ssh
cargo make ssh -- -i ~/.ssh/id_ed25519
```

That is `ssh -p 2222 root@127.0.0.1` (`OATH_SSH_PORT` overrides 2222).

Port: `OATH_SSH_PORT` (default 2222), forwarded to guest 22.

Empty `authorized` means nobody can log in. Undo restores the last
keys. Host keys persist across undo of key lists (same machine).

A network install should **inject owner public keys** into `ssh:local`
and let the target generate host keys. Do not bake a login private key
into the image.
