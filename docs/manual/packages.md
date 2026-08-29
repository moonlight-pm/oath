# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/`. `/bin` is a **symlink farm**, not an unpack
target. There is no `apt` and no `oath install`.

## What ships

| Id | Default | Notes |
|----|---------|-------|
| `pkg:hello` | `present: false` | Canary. Store is on the image; apply links `/bin/hello`. |

`/bin/hello` prints `hello`. The symlink target is
`/oath/store/pkg/hello/bin/hello`. Do not exec from the store; `/bin`
is how you run what is installed.

Busybox, `btrfs`, and `oath` are **not** package objects yet.

## Install / remove

```
oath ls --kind pkg
oath get pkg:hello
oath set pkg:hello present=true
oath apply
hello
readlink /bin/hello

oath set pkg:hello present=false
oath apply
oath undo
```

`present=false` removes **this object’s** `/bin` links. The store tree
stays (re-install needs no network). Apply refuses to clobber a `/bin`
name it does not own.

Takes effect on apply. Reboot is not required; it only proves the
symlink survived on `@`.
