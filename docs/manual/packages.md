# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/`. `/bin` is a **symlink farm**, not an unpack
target. There is no `apt` and no `oath install`.

## What ships

| Id | Default | Removable |
|----|---------|-----------|
| `pkg:busybox` | `present: true` | no — applets are this one object |
| `pkg:btrfs` | `present: true` | no |
| `pkg:oath` | `present: true` | no |
| `pkg:dropbear` | `present: true` | no |
| `pkg:glibc` | `present: true` | no — GNU C runtime for River and Sola; not loaded by PID 1 |
| `pkg:river` | `present: true` | yes — patched River; `/bin/river` |
| `pkg:sola` | `present: true` | yes — session stack + `sola-terminal` + `sola-browser` + `sola-workspaces`; `/bin/sola-bus` and siblings including `sola-session`, `sola-terminal`, `sola-browser`, `sola-workspaces`, `solactl`, and `tmux` (C.UTF-8 locale-archive; CEF under `cef/`; SF Pro Text + Iosevka Term Slab, with Inter / JetBrains Mono fallbacks) |
| `pkg:grok` | `present: true` | yes — borrowed static-pie Grok ELF; `/bin/grok`. Updater off (`GROK_DISABLE_AUTOUPDATER`). State is `/home/.grok`, not the payload. |
| `pkg:hello` | `present: false` | yes — canary |
| `pkg:fetchme` | `present: false`, `url` | yes — wget canary |

`/bin/hello` prints `hello`. The symlink target is
`/oath/store/pkg/hello/bin/hello`. Do not exec from the store; `/bin`
is how you run what is installed.

`present=false` on a non-removable package is **refused** (not
`--confirm`). PID 1 stays at `/lib/oath/init`; it is not a
package.

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

`url` on a `pkg` object: if `present` and the store file is missing,
apply wget’s the URL into the store then links. The appliance canary
is `pkg:fetchme` (`http://10.0.2.2:18765/fetchme` on QEMU user net).
