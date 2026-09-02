**Date:** 2026-09-02
**Status:** target (freeze)
**Implementation:** shipped (eighth ELF + `solactl` in `pkg:sola`)
**Dogfood:** canto live (`/bin/sola-workspaces`, `/bin/solactl`). QEMU
after next `cargo make build`.
**Gaps:** no `git` / `grok` on the guest yet (spawn sibling and agent
panes wait); Oath tree not on canto; rustc/host toolchain is T24 inner
loop
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Kit workspaces on Oath (`sola-workspaces`)

T26 packed Terminal, T28 Browser. The launcher already lists Workspaces
at `env::bin_path("sola-workspaces")`. This freeze packs that ELF and
**`solactl`** (call-plane CLI, including `solactl workspaces`) — **not**
git, **not** grok, **not** a split of `pkg:sola`.

This is the first step of **Oath as the development seat** (T24 identity
unchanged: one `pkg:sola` blob, apply/undo, no `/opt/sola` on Oath).
The inner loop (build a tree on canto, self-watch) is still out.

---

## Locks this freeze owns

- **Eighth ELF in the one `pkg:sola` blob** (T24). Store
  `/oath/store/pkg/sola/`; `/bin/sola-workspaces` via the symlink farm.
  Do not add `pkg:sola-workspaces`. Do not split the blob.
- **`solactl` is a helper in that same blob**, like tmux. `/bin/solactl`.
  Workspaces CLI needs `sola-call` (already a `svc`). Do not start
  `dbus-daemon` for `solactl media`.
- **PID 1 does not supervise Workspaces.** `sola-session` launches it
  (direct spawn). No new `svc:*`. tmux socket `sola-ws` is the app's,
  not a catalog service.
- **No git/grok in this slice.** Spawn sibling and Grok panes need
  those binaries later (borrowed prebuilts or helpers in the blob).
  The window must still open.

---

## Courage test (this slice)

On canto (and QEMU after rebuild):

1. `test -x /bin/sola-workspaces` and `test -x /bin/solactl`.
2. After the session stack is up: spawn `/bin/sola-workspaces`;
   `pidof sola-workspaces` is non-empty. `pgrep -x sola` stays empty.
3. Serial and SSH still work.

---

## Out

- `git`, `grok`, rustc, the Oath git checkout on canto
- `pkg:sola-workspaces` / splitting `pkg:sola`
- Nested `crates/sola`, udevd, dbus-daemon
- Other remaining kit apps (mail, wrapper, settings, …)
