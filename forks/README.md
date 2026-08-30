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
| `river/` | [riverwm/river](https://github.com/riverwm/river) | [moonlight-pm/oath-river](https://github.com/moonlight-pm/oath-river) | River 0.4.5 + Sola’s three River patches as commits |
| `wlroots/` | [wlroots](https://gitlab.freedesktop.org/wlroots/wlroots) | [moonlight-pm/oath-wlroots](https://github.com/moonlight-pm/oath-wlroots) | Sola’s screencopy cursor patch as a commit |

**Put a tree here** when we patch it or reshape it for Oath.

**Do not** put unmodified busybox, mesa, or a random tarball here.
Those stay `image/` (borrow) or a `pkg.url` (T20). First-party
programs we wrote (`hello`, `fetchme`, later canaries) live in
[`apps/`](../apps/README.md).
