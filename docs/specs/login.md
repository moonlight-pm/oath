# Login and the seat

The desk's password and who has the screen and keyboard (R42). Oath's
half of a design made with Sola (Sola's repo, branch `next`:
`docs/specs/2026-10-29-secure-access-design.md`): Oath keeps the
password, the auto-login setting and the seat; Sola builds the lock
screen (`sola-lock`, on ext-session-lock-v1) and Settings → Login
against the contract below.

**Status:** built 2026-10-29. In QEMU the `login-and-seat` case checks
`svc:login`, the hash, the delay, auto-login and both turns of the seat;
the console's prompt was tried by hand there (a wrong password refused,
the right one opening the shell, `oath desk` back to the desk in 0.2 s).
Not yet run on a machine; Sola's side is not built.
Every part is optional, and the default is the desk as it was: no
password, the desk starts signed in, nothing locks.

## The contract (built)

| | |
|---|---|
| Service | `svc:login`, root: `/lib/oath/login`, in `base` |
| Socket | `/oath/run/login.sock`, mode 0666; answers only `home` (uid 1) and root, by `SO_PEERCRED` |
| Wire | one JSON object a line; each request gets one line back; a connection may carry several |
| Password | `home`'s, Argon2id (64 MiB, 3 passes, a 16-byte salt from `/dev/urandom`) in `/oath/login/password`, mode 0600 in a 0700 directory, root's; none at first |
| Auto-login | `host:local.autologin`, a typed catalog field, `true` when absent |
| Seat | `/oath/run/seat`: `desk` or `console`; the desk's when absent, so at every boot |
| Commands | `oath password` (`--status`), `oath desk`, `oath desk stop` |

The calls:

| Request | Answer |
|---|---|
| `{"call":"status"}` | `{"ok":true,"password":false,"autologin":true,"seat":"desk"}` |
| `{"call":"check","password":"…"}` | `{"ok":true,"right":true}`, or `"right":false` |
| `{"call":"set","old":"…","new":"…"}` | `{"ok":true}`; an empty `new` removes the password; `old` only while one is set |
| `{"call":"autologin","on":false,"password":"…"}` | `{"ok":true}`; `password` only while one is set |
| `{"call":"desk","up":false}` | `{"ok":true,"seat":"console"}`: the desk ended on purpose; `"up":true` gives the seat back |

A refusal is `{"ok":false,"error":"…"}` (a wrong `old` or `password`
is `"wrong password"`). Every wrong password (in `check`, `set` and
`autologin`) is answered after a delay that doubles with each one in a
row, 1 s, 2 s, 4 s and on to 30 s, and a right one ends it. Checks run
one at a time, each after the delay the last wrong one set, so the
delay holds for every caller at once. `status` and `desk` never wait.

**Sola's side.** Sola reads `status` when the desk starts and locks at
once when `autologin` is false and `password` is true; its lock screen
unlocks on `check`; Settings → Login uses `set` and `autologin`; Exit
Sola sends `desk` with `"up":false` (or runs `oath desk stop`) as its
last step. Nothing else tells Oath that the desk ended on purpose: a desk
that exits or crashes without it is restarted as before.

## The seat (built)

The seat is tty1, its screen and keyboard. A `svc` may name its turn on
it, `seat: "desk"` or `seat: "console"`; PID 1 runs such a service only
while that owner has the seat, and holds it otherwise: stopped, its
actual `state: "held"` with a `reason`, and `oath apply` counts held as
converged. A service with no `seat` runs either way. In `desk`,
`svc:rivulet`, `svc:sola-river`, `svc:sola-shell`, `svc:sola-session`
and `svc:sola-kvm` are the desk's; `svc:screen` (`/lib/oath/screen-login`,
a login shell as `home` on tty1) is enabled and the console's. Sola's
bus, call plane and tmux servers have no seat, so every shell they hold
lives through a turn of the console.

`desk` with `"up":false` writes `console` to `/oath/run/seat` and asks
PID 1 to converge: the desk's services stop, rivulet gives up the VT,
and the console starts on tty1 (text mode, in front). `oath desk` writes
`desk`: the console's shell ends (with whatever ran in it: `screen-login`
is its parent, so stopping it hangs up the terminal), and the desk's
services start again, those that had exited or given up included. A boot
always starts with the desk.

## The console and auto-login (built)

`svc:screen` reads `host:local.autologin`. On, it opens a shell as
`home` at once. Off, with a password set, it asks for it
(`Password for home on <host>:`, not echoed) and checks it with
`svc:login`; while `svc:login` does not answer it waits and opens no
shell. Off with no password set, there is nothing to ask. `oath desk`
from that shell gives the seat back. The same holds on `base`, where
`svc:screen` has no seat and is the screen's shell all the time.

`svc:login` sets auto-login with `oath set host:local autologin=…` and
`oath apply host:local`, as `svc:login` in the apply log (`actor.via`),
so it is a generation like any other change and `oath undo` takes it
back. It refuses while `host:local` has other changes nobody applied,
since that apply would carry them too. Root may set the field as any
other, without the password.

## What the password protects, and what it does not

Three setups, from the default:

| Setup | At boot | The lock (Super+L) | After Exit Sola |
|---|---|---|---|
| No password (default) | the desk, unlocked | none to unlock with | a shell as home |
| A password, auto-login on (Joshua's) | the desk, unlocked | locks; the password unlocks | a shell as home, not asked |
| A password, auto-login off | the desk, locked by Sola | locks; the password unlocks | the console asks for it |

The password guards the **seat**: a desk that is locked, and, with
auto-login off, the console and a desk that starts. rivulet keeps the
screen blank and takes no input for anything but the lock screen while
locked, and a lock screen that dies leaves it locked
(`apps/rivulet/src/lock_manager.c`). It does not guard the account or
the machine:

- **`sudo` asks no password.** Anything running as `home` is root: a
  terminal on an unlocked desk, an SSH session, a program `home` runs.
  Locking keeps a person at the keyboard out of the session; it does
  nothing once they are in it.
- **SSH** takes keys only; the password plays no part in it.
- **A restart of the desk.** With auto-login on, a desk that crashes,
  is killed or comes back after a reboot starts unlocked: the lock lasts
  only as long as the desk runs. Every SysRq key is on (the kernel's
  default, `kernel.sysrq` 1), so anyone at the keyboard can kill the desk
  (Alt+SysRq+K) or reboot (Alt+SysRq+B). With auto-login off, the desk
  comes back locked; from rivulet's start until Sola's lock screen locks
  the desk is open, and keeping that short is Sola's.
- **The serial console.** Where `svc:serial` finds a UART it is a root
  shell with no password (break-glass). canto has none; `oath logs
  svc:serial` says `no serial console` on a machine without one.
- **The disk is not encrypted.** Whoever boots another system (a USB
  stick from the firmware's boot menu) reads and writes everything,
  `/home` and the hash too; the hash costs 64 MiB and three passes a
  guess.
- **The boot menu** (`oath-efi`) has no password; every generation it
  offers has the same accounts.
- **The delay** lasts while `svc:login` runs; restarting it (root
  only) starts the count again.

## Will be

- Nothing here touches `sudo`: whether a set password should make it
  ask (and how agents then get root over SSH) is a question for Joshua.
- `kernel.sysrq` and other kernel settings have no catalog field yet.
- Machines installed before R42 get `/lib/oath/login` with an update,
  but not `svc:login` or the `seat` fields (an update does not move
  `svc` specs, CURRENT item 1); the manual has the commands.
