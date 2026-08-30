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
| `sola/` | [moonlight-pm/Sola](https://github.com/moonlight-pm/Sola) | `moonlight-pm/oath-sola` (not added yet) | Oath-compat: catalog, `svc:*`, no nested supervisor |
| `river/` | [river/river](https://codeberg.org/river/river) | `moonlight-pm/oath-river` (not added yet) | River 0.4.x + Sola’s three River patches as commits |
| `wlroots/` | [wlroots](https://gitlab.freedesktop.org/wlroots/wlroots) | `moonlight-pm/oath-wlroots` (not added yet) | Sola’s screencopy cursor patch as a commit |

Until those remotes exist, the patches still live in the daily-driver
Sola tree at `nix/patches/`. Do not copy them into Oath as a second
overlay.

**Put a tree here** when we patch it or reshape it for Oath.

**Do not** put unmodified busybox, mesa, or a random tarball here.
Those stay `image/` (borrow) or a `pkg.url` (T20).
