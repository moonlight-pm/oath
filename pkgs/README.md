# pkgs/

Every package that ships in Oath is built from source here (R7 in
[../docs/decisions.md](../docs/decisions.md)).

| File | What |
|------|------|
| `<name>/pkg.toml` | version, licence (SPDX), description, upstream sources pinned by sha256, the packs it builds with (`build`) and needs at runtime (`run`) |
| `<name>/build.sh` | the build, run by `oath build` in the sandbox: no network, nothing from the build host, only the declared packs |
| `<name>/*` (other files) | configs and patches; the plan sees them under `$FILES` |
| `<name>/<name>.plan` | generated: the exact plan that was built (source pack and every build pack pinned by hash) |
| `lock.json` | what each name resolves to: version, licence, how it was built, plan hash, product hash |
| `bootstrap.sh`, `bootstrap.nix` | stage 0 and stage 1 on the build host |

## Bootstrap

`cargo make bootstrap` (hours) builds the toolchain from the pinned
sources in `llvm/pkg.toml`, `busybox/pkg.toml` and `make/pkg.toml`:

1. **Stage 0**, with the build host's gcc (`nix-shell bootstrap.nix`):
   clang and lld targeting `x86_64-unknown-linux-musl`, and a sysroot of
   Linux UAPI headers, musl, compiler-rt builtins, libunwind, libc++abi
   and libc++. Stage 0 runs only on the build host and never ships.
2. **Stage 1**, with stage 0: static musl clang, lld and LLVM tools plus
   the same sysroot (`pkg:llvm`), and static busybox and GNU make. Their
   `cc` / `c++` find the sysroot through `bin/clang.cfg` and link
   statically by default.

`pkg:llvm` stays a bootstrap product until cmake, ninja and python are
packs and LLVM can be rebuilt by a plan.

## Stage 2

`cargo make pkgs [names]` builds packages in dependency order. For each:
fetch and verify the sources; put them (and the package's extra files)
in a `pkg:<name>-src` pack; write `<name>.plan` with the source pack and
the build packs from `lock.json` pinned by hash; run `oath build` in the
catalog at `.cache/oath`; record the product in `lock.json`. A package
whose plan has not changed is not rebuilt. busybox and make are rebuilt
here by their stage-1 selves; run `cargo make pkgs` again and they
rebuild with the stage-2 tools.

The generated script starts with `SRC` (the tarballs), `FILES`,
`SOURCES`, `VERSION`, `JOBS`, `LIVE` (the slot path the pack is used at,
`/oath/store/pkg/<name>/live`), `CC=cc` and friends, and `CPPFLAGS`,
`LDFLAGS` and `PKG_CONFIG_PATH` covering every library pack it builds
with. Library packs install under `$LIVE` so their `.pc` files work in
the next sandbox. Write the result to `/out`.

`cargo make pkgs --list` shows every package, its licence and its pins.
`cargo make build` packs the image from the products in `lock.json`.
