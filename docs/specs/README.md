# Specs

Each spec describes one domain of Oath core: what is built, the known
defects, and the target design. Code wins over a spec; when they
disagree, fix the spec in the same change. Decisions behind a spec live
in [../decisions.md](../decisions.md).

| Spec | Domain | Status |
|------|--------|--------|
| [scope.md](scope.md) | What is core and what is a payload; the Steam and Minecraft desk requirement | built |
| [catalog.md](catalog.md) | Objects, verbs, apply, undo, generations, INDEX | partly built |
| [packages.md](packages.md) | `pkg`, the content-addressed store, needs, origins, advisories, the native rule | partly built |
| [builds.md](builds.md) | `plan`, `oath build`, the sandbox, the native toolchain | built (desk packages: phase 8) |
| [services.md](services.md) | `svc` and PID 1 | partly built |
| [network-and-ssh.md](network-and-ssh.md) | `net`, `ssh`, the `home` account, `sudo` | partly built |
| [devices.md](devices.md) | `dev`, modules, firmware | partly built |
| [boot-and-install.md](boot-and-install.md) | The one kernel, `oath-efi`, profiles, overlays, install | partly built |
| [desk.md](desk.md) | The graphical session: rivulet, wlroots, Mesa, Xwayland, the seat, Sola | running on canto |
| [login.md](login.md) | `home`'s password, auto-login, the seat's turns (desk, console), `svc:login`'s contract with Sola (R42) | built (QEMU) |
| [steam.md](steam.md) | Steam: the recipe, its sandbox, the i386 packages | partly built (canto) |
| [agent.md](agent.md) | `oath mcp`: the verbs as MCP tools over SSH; the real-model acceptance check | partly built (phase 7) |

Each spec has a **Status** line, then sections labelled *built*, *known
defects* and *target*. "Is" and "will be" never share a paragraph.

The dated freezes these replace (T1–T44) are in git history up to tag
`t37-t39-final`.
