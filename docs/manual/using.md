# Using `oath`

`oath` is the only admin surface. Humans and agents use the same verbs.
Do not edit `/etc` or hunt random files.

## Start here

```
oath                 short INDEX
cat /oath/INDEX.md   full INDEX (generated, live)
```

`--json` prints the same facts as JSON. Failures include a **hint**
pointing at the next page (`oath ls`, `oath schema <kind>`, INDEX safety).
On the Sola desk, `/bin/sola-oath` is the same verbs for `pkg:*`
(Present toggle, Apply, Undo). Filter the list; click a pack for description, project page,
**Needs** (with realization pins), and **Used by**. Present stays
pinned above the tree.
Open project is `xdg-open`.
Toggle off stages dependents too; toggle on stages missing needs.
Apply still goes through `oath`. Canto has the ELF live-copied;
launcher **Oath** ships when `sola-shell` is repacked.

Catalog root is `/oath`. `OATH_ROOT` / `--root` is for tests on the
build host, not the appliance.

## Verbs

| Verb | Does |
|------|------|
| `oath ls` | List object ids. `--kind host` (etc.) to filter |
| `oath schema [kind]` | List kinds, or one kind’s prose + JSON Schema |
| `oath get <id>` | Desired + actual + status. `--desired` / `--actual` to clip |
| `oath set <id> k=v` | Write **desired** only. Does not converge |
| `oath diff [id]` | Drift between desired and actual (or last applied, for `svc`) |
| `oath apply [id...]` | Snapshot, then converge listed ids or all drift |
| `oath undo` | Restore the last apply’s generation |
| `oath log` | Apply log (JSON lines) |
| `oath fmt <path>` | Canonical `plan.plan` to stdout (`-w` in place) |
| `oath build plan:<name>` | Isolated netless build of a pinned plan. Does not link `/bin` |
| `oath build --file <path>` | Same, ad-hoc (does not store the plan) |

`set` is staging. Nothing is live until `apply`.

Ids are `kind:name` — `host:local`, `svc:serial`, `snap:current`,
`pkg:hello`, `plan:hello`, `net:net0`, `ssh:local`, `dev:card0`, `dev:kbd0`.

`oath set plan:<name> --from-file ./hello.plan` lints, stores, and pins.
Plans must already be canonical (`oath fmt -w`). Build never activates
`/bin`; that is still `oath set pkg:<name> present=true` then apply.

```
oath fmt -w apps/hello.plan
oath build --file apps/hello.plan
oath set plan:hello --from-file apps/hello.plan
oath build plan:hello
```

`apps/hello.plan` compiles `apps/hello-src/hello.c` with pinned
`pkg:cc` and `pkg:busybox`. The product is a new `pkg:hello`
realization. A script that calls `musl-cc` without declaring
`pkg:cc` fails inside the sandbox.

## Safety

`apply` takes a filesystem snapshot first.

- **mutate** — hostname, service enable, ordinary apply. Snapshot and go.
- **confirm** — halt, reboot, picking an old boot generation (not `undo`).
  `oath apply` without `--confirm` exits **3** and explains. Do not pass
  `--confirm` unless the owner asked for that class of change.

```
oath set host:local hostname=atlas
oath diff
oath apply

oath set host:local power=reboot
oath apply              # refused (exit 3)
oath apply --confirm    # reboots

oath set host:local power=halt
oath apply --confirm    # powers off; QEMU should exit
```

The Sola flower menu **Restart Computer** / **Shut Down** run those
same verbs (`sudo oath apply --confirm`). The click is the owner's
confirm.

After a reboot apply, desired `power` is set back to `run` so the box
does not loop.

Graphical desk (`host:local.session`, default `sola`):

```
oath set host:local session=omarchy
oath apply              # refused (exit 3)
sudo oath apply --confirm    # stops River/Sola, enables Hyprland + omarchy-shell
oath undo                    # previous session
```

Needs `--confirm` (kills the graphical session). PID 1 waits 3 s for DRM
release; reboot if the incoming compositor still loses the race.
`svc:sola-kvm` stays enabled but its Wayland connection dies with the
outgoing compositor — bounce it (`restart=always`) so the pointer
attaches to the incoming desk. River and
Hyprland cannot both be enabled. Bits for both desks stay packed (`pkg:sola`
and `pkg:hyprland` / `pkg:quickshell` / `pkg:omarchy`). Omarchy’s bar is
`svc:omarchy-shell` (`/bin/quickshell -p $OMARCHY_PATH/shell`). On the
Omarchy desk (Hyprland 0.52 conf, not Lua): **Super+Return** opens
Terminal (`pkg:foot`; sola-terminal if foot is missing), **Super+Space**
toggles the Omarchy menu, **Super+K** lists keybindings (searchable
overlay; Enter runs the chord), **Super+Shift+Return** opens sola-browser,
**Super+Ctrl+C** is Omarchy’s capture menu (the documented fallback
when there is no Print Screen — Mac keyboards). **Print** is the
screenshot picker if the keyboard has that key; **Super+Print** is
the color picker. Super+Shift+1–0 moves a window to that Hyprland
workspace (Omarchy desktop groups, not Sola Workspaces). Files land
in `~/Pictures`. In the picker, Return captures the highlighted
window and Ctrl+Return the whole output.
`open path.png` (or `xdg-open`) opens **sola-paint**. **Super+W** closes the focused window, Super+arrows move focus,
Super+1–0 switch workspaces. Type is JetBrainsMono NF (fontconfig
aliases Omarchy’s `monospace` / `JetBrainsMono Nerd Font` name);
icons from that face plus `omarchy.ttf`. The pointer is **Yaru**
(same default as the Omarchy ISO, not Hyprland’s built-in cursor
and not Sola’s McMojave). `ssh home@canto
/bin/busybox ash` for scripts (`thoxa -c` echoes a quoted script).

## Who you are

Serial login is **root** (break-glass) when `svc:serial` is on.
Daily SSH is **home** (uid 1, `HOME=/home`). `sudo` (no password)
becomes root. Groups are `root` and `home` only. The apply log
records uid and tty. Catalog env (`host:local.env`) is injected by
PID 1 and written to root-owned `/etc/profile`. `oath apply` does
not write `$HOME/.profile`.
