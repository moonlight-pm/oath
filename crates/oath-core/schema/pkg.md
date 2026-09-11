# pkg

A package. Bits live under `/oath/store/pkg/<name>/<hash>/`. `/bin` is
a symlink farm, not an unpack target. There is no `apt` and no
`oath install`. Name is a slot; hash is the bits.

**When to use:** install or remove a package (`present`); pin a
realization (`hash`).

**When not:** do not `ln` or `rm` in `/bin`. Do not exec from the
store. Do not set `present=false` on `pkg:busybox`, `pkg:btrfs`,
`pkg:oath`, `pkg:dropbear`, or `pkg:glibc`.

## Fields

- `present` — `true` links `store/.../<hash>/bin/*` into `/bin`.
  `false` removes **this object’s** links. Store stays so re-install
  needs no network.
- `hash` — optional pin. Realization id is `sha256-` plus 64 lowercase
  hex of the pack tree (`oath-tree-v1` in this schema). Apply verifies
  bytes match; it does not choose. Empty hash is discovery: unique
  on-disk tree or a fetch reports `actual.hash`. Two hashes of one
  name can sit on disk; only the pin is linked.
- `url` — optional. If set and the store is missing, apply wget’s it
  then links. A `.tar` (or `.tar.gz`) is a pack tree (hash is of the
  tree, not the tar). An origin prefix plus `hash` fetches
  `{url}/pkg/{name}/{hash}.tar` — object storage is just HTTPS. A
  single file (canary `pkg:fetchme`) becomes `bin/<name>`. Local store
  wins if the realization is already there.
- `requires` — optional hardware the bits need. `drm_modifiers: true`
  means apply `present=true` is refused unless a connected GPU exports
  Vulkan WSI DRM format modifiers (AMD GFX6–8 and virtio-gpu do not).
  `pkg:gamescope` always has this requirement. Uninstall is always
  allowed. `sola-arcade` lives in `pkg:sola`; apply skips that `/bin`
  link on the same GPUs rather than refusing the whole Sola blob.

The slot also has `live` → `<hash>` and compat symlinks (`bin` →
`live/bin`, …) so RPATH/env that still name
`/oath/store/pkg/<name>/lib` keep working. Those wrappers are not
part of the hash.

Actual also has `links` (basenames in `/bin`), `removable`, live
`hash`, and `realizations` (other trees for this name: hash, linked,
`bin/` names, first lines of pack `INDEX.md`). If `removable` is
false, `present=false` is refused (not `--confirm`).

Safety: `mutate`. Apply snapshots, then converges links.

Optional store files (not schema fields):
`libexec/oath-backup-quiesce` and `libexec/oath-backup-thaw`. The
off-box backup helper runs them on **present** packs before/after the
generation snapshot. Missing = skip.

## Example

```
oath ls --kind pkg
oath get pkg:hello
oath set pkg:hello present=true
oath apply
hello
oath set pkg:hello present=false
oath apply
oath undo
```

The appliance ships `pkg:busybox`, `pkg:btrfs`, `pkg:oath`,
`pkg:dropbear`, and `pkg:glibc` already present (not removable),
`pkg:river` present (removable), `pkg:sola` present (removable; session
ELFs plus `sola-terminal` and `tmux`), `pkg:grok` present (removable;
borrowed static-pie; updater off), `pkg:git` and `pkg:curl` present
(removable), and
`pkg:hello` absent until you apply. `/bin/hello` prints `hello`.
Busybox applets are one package. `pkg:glibc` is the GNU C runtime
for glibc payloads (River, Sola, rustc). Never load it into musl PID 1.
`pkg:cc` / `pkg:rustc` / `pkg:cmake` / `pkg:pkg-config` are the guest
toolchain (T35). `pkg:bash` / `pkg:xwayland` / `pkg:gamescope` /
`pkg:steam` are the Arcade runtime (T37). `pkg:gamescope` is
`present: false` until a GPU with DRM modifiers applies it;
`sola-arcade` is not linked on cards that cannot run the nest.
`pkg:bluez` is the system D-Bus plus BlueZ (`/bin/dbus-daemon`,
`/bin/bluetoothd`). Session bus / MPRIS stay out.
