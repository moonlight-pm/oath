# Packages

A package is a `pkg:*` catalog object. Bits live under
`/oath/store/pkg/<name>/`. `/bin` is a **symlink farm**, not an unpack
target. There is no `apt` and no `oath install`.

## What ships

| Id | Default | Removable |
|----|---------|-----------|
| `pkg:busybox` | `present: true` | no — applets are this one object (includes `/bin/vi`) |
| `pkg:btrfs` | `present: true` | no |
| `pkg:oath` | `present: true` | no |
| `pkg:dropbear` | `present: true` | no — dropbear, dropbearkey, scp, sftp-server |
| `pkg:glibc` | `present: true` | no — GNU C runtime for River and Sola; not loaded by PID 1 |
| `pkg:river` | `present: true` | yes — patched River; `/bin/river` |
| `pkg:sola` | `present: true` | yes — session stack + `sola-terminal` + `sola-browser` + `sola-workspaces` + `sola-kvm`; `/bin/sola-bus` and siblings including `sola-session`, `sola-terminal`, `sola-browser`, `sola-workspaces`, `solactl`, `sola-kvm`, and `tmux` (C.UTF-8 locale-archive; CEF under `cef/`; SF Pro Text + Iosevka Term Slab, with Inter / JetBrains Mono fallbacks) |
| `pkg:grok` | `present: true` | yes — borrowed static-pie Grok ELF; `/bin/grok`. Updater off (`GROK_DISABLE_AUTOUPDATER`). State is `/home/.grok`, not the payload. |
| `pkg:git` | `present: true` | yes — borrowed Git; `/bin/git`. HTTPS via `git-remote-http` + CA bundle in the store. |
| `pkg:curl` | `present: true` | yes — borrowed static musl curl; `/bin/curl`. CA bundle in the store. |
| `pkg:pipewire` | `present: true` | yes — PipeWire + WirePlumber + pipewire-pulse + alsa-lib + libpulse; `/bin/pipewire`, `/bin/wireplumber`, `/bin/pipewire-pulse`, `/bin/wpctl`, `/bin/pw-dump`, `/bin/pw-cat`. Menubar volume talks to this (`pw-dump` / `wpctl` / `pw-cat`). sola-spotify (not packed) needs the Pulse socket + those libs at runtime. |
| `pkg:thoxa` | `present: true` | yes — sister compiler + session REPL; `/bin/thoxa` is the `home` login shell (wrapper + `libexec/thoxa`) and the `$SHELL` for sola-terminal / workspaces tmux. Interactive TTY is an emacs line editor with Tab complete (Thoxa `ee2e3cf`, canto this boot). Seat rc is `~/.config/thoxa/shell.thx` — do not copy a NixOS `path()` list (`path` *replaces* PATH; import `std/prompt`, not `std/Prompt`). Root/serial stay busybox `/bin/sh`. `thoxa -c` is libtcc (no guest `cc`); `thoxa build` still needs `cc`. |
| `pkg:hello` | `present: false` | yes — canary |
| `pkg:fetchme` | `present: false`, `url` | yes — wget canary |

`/bin/hello` prints `hello`. The symlink target is
`/oath/store/pkg/hello/bin/hello`. Do not exec from the store; `/bin`
is how you run what is installed.

`present=false` on a non-removable package is **refused** (not
`--confirm`). PID 1 stays at `/lib/oath/init`; it is not a
package.

Optional backup hooks in the store tree (T32/T33), not catalog
fields:

```
/oath/store/pkg/<name>/libexec/oath-backup-quiesce
/oath/store/pkg/<name>/libexec/oath-backup-thaw
```

`backup-send` runs quiesce on **present** packs that shipped the
executable, snapshots, then thaw. Missing hook: crash-consistent
only. Mention the hook in that pack’s `INDEX.md` if you ship one.

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

`pkg:pipewire` is the seat audio graph, not a Sola blob. Without it the
shell **hides** the volume chip. There is no udevd, so HDMI cards are
not auto-enumerated; canto pins Intel PCH analog as **Built-in Audio**
(`hw:0,0`) in the packed `pipewire.conf.d`. `wpctl status` as `home`
(`XDG_RUNTIME_DIR=/run/user/1`) is the check.

`url` on a `pkg` object: if `present` and the store file is missing,
apply wget’s the URL into the store then links. The appliance canary
is `pkg:fetchme` (`http://10.0.2.2:18765/fetchme` on QEMU user net).
