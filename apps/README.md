# apps/ — first-party programs that ship as `pkg:*`

We wrote these. They are not the OS control plane (`crates/`), not
patched upstream (`forks/`), and not borrowed prebuilts (`image/`).

Each directory is the store tree for `pkg:<name>`:

```
apps/<name>/bin/<name>  →  /oath/store/pkg/<name>/<hash>/bin/<name>
```

That layout **is** the pack. Oath does not ship a recipe language;
authors produce this tree however they want. Realization id is
SHA-256 of `oath-tree-v1`; store is `/oath/store/pkg/<name>/<hash>/`;
pin on `desired.hash`. Host cache:

```
cargo make store --name hello --from apps/hello --tar
cargo make publish --dest <dir>
```

writes `.cache/oath/store/pkg/hello/<hash>/` and an optional `.tar`,
then copies the tars into an origin tree at `<dir>`.

Optional, next to `bin/`:

```
apps/<name>/libexec/oath-backup-quiesce
apps/<name>/libexec/oath-backup-thaw
```

`/lib/oath/backup-send` runs those executables on present packs
around its snapshot. No hook is the default (crash-consistent).

| Path | Catalog | Notes |
|------|---------|--------|
| `hello/` | `pkg:hello` | example pack tree for `cargo make store` (not in the image) |
| `hello-src/` | `pkg:hello-src` | C source for the compile plan (`apps/hello.plan`) |
| `hello.plan` | `plan:hello` | compile `hello.c` with `pkg:gcc`, `pkg:glibc`, `pkg:bash`, `pkg:coreutils` and what they run with |
| `reloc-src/` | `pkg:reloc-src` | upstream prefix tree for the relocate plan |
| `reloc.plan` | `plan:reloc` | relocate `hello-1.0/hello` into `/out/bin/reloc` |
| `rivulet/` | `pkg:rivulet` (to come) | the compositor: River 0.4.5 ported to C (R19); see [rivulet/README.md](rivulet/README.md) |
| `fetchme/` | `pkg:fetchme` | single-file payload the harness serves; fetch cases make `pkg:fetchme` with `url` `http://10.0.2.2:18765/fetchme` |

The pins in `hello.plan` and `reloc.plan` name products in
`pkgs/lock.json` (as of 2026-10-18); re-pin them when those change.
Build them on the build host with `cargo make store` for the source
pack, then `oath --root .cache/oath build --file apps/hello.plan`.
