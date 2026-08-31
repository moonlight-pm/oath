# forks/ — source trees we maintain

Build-time git submodules of **product forks**. Not the live package
store (that is `/oath/store` + `pkg.url`, T20). Not borrowed
prebuilts (`image/`). Not a `pkg:*` object.

Clone:

```
git clone --recurse-submodules https://github.com/moonlight-pm/oath.git
```

| Path | Upstream | Our remote | Why it is here |
|------|----------|------------|----------------|
| `sola/` | [moonlight-pm/Sola](https://github.com/moonlight-pm/Sola) | [moonlight-pm/oath-sola](https://github.com/moonlight-pm/oath-sola) (private) | Oath-compat: catalog, `svc:*`, no nested supervisor |
| `river/` | [riverwm/river](https://github.com/riverwm/river) | [moonlight-pm/oath-river](https://github.com/moonlight-pm/oath-river) | River 0.4.5 + Sola’s three River patches as commits |
| `wlroots/` | [wlroots](https://gitlab.freedesktop.org/wlroots/wlroots) | [moonlight-pm/oath-wlroots](https://github.com/moonlight-pm/oath-wlroots) | Sola’s screencopy cursor patch; libinput path fallback when udev finds no devices |

**Put a tree here** when we patch it or reshape it for Oath.

**Do not** put unmodified busybox, mesa, or a random tarball here.
Those stay `image/` (borrow) or a `pkg.url` (T20). First-party
programs we wrote (`hello`, `fetchme`, later canaries) live in
[`apps/`](../apps/README.md).

## Sola: generic fixes vs Oath-compat

`forks/sola` is [oath-sola](https://github.com/moonlight-pm/oath-sola).
The NixOS daily driver is [Sola](https://github.com/moonlight-pm/Sola).
T23: pull Sola into the fork; never push **Oath-compat** the other way.

Oath dogfood will find real Sola bugs (the GPU just hid them). Split
commits so those can move.

| Class | What | Where |
|-------|------|--------|
| **Sola-generic** | Overlay sizing, kit, bridge, anything that belongs on the NixOS desk | Cherry-pick (or land first) on `moonlight-pm/Sola`, then merge Sola `master` into `oath-sola` |
| **Oath-compat** | `/oath` vs `/opt/sola`, refuse `crates/sola` when `/oath/INDEX.md` exists, PID 1 env, packing | `oath-sola` only |

```sh
cd forks/sola
git remote add sola git@github.com:moonlight-pm/Sola.git   # once
git fetch sola
git checkout -b from-oath/<topic> sola/master
git cherry-pick -x <generic-commit>
git push sola from-oath/<topic>
# PR (or merge) onto Sola master, then:
git checkout master
git fetch sola
git merge sola/master
```

Prefer two commits when a change is mixed (generic first, then
Oath-compat) so the cherry-pick is clean.
