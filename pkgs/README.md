# pkgs/

Every package that ships in Oath is built from source here (R7 in
[../docs/decisions.md](../docs/decisions.md)).

| File | What |
|------|------|
| `<name>/pkg.toml` | version, licence (SPDX), description, upstream sources pinned by sha256, the packs it builds with (`build`) and needs at runtime (`run`) |
| `<name>/build.sh` | the build, run by `oath build` in the sandbox: no network, nothing from the build host, only the declared packs |
| `<name>/*` (other files and directories) | configs and patches; the plan sees them under `$FILES` |
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

After that, `llvm/build.sh` is stage 1 as a plan: the previous
`pkg:llvm`, with the `cmake`, `ninja` and `python` packs, builds a fresh
sysroot and the tools in the sandbox.

## Stage 2

`cargo make pkgs [names]` builds packages in dependency order. For each:
fetch and verify the sources; put them (and the package's extra files)
in a `pkg:<name>-src` pack; write `<name>.plan` with the source pack and
the build packs from `lock.json` pinned by hash; run `oath build` in the
catalog at `.cache/oath`; record the product in `lock.json`. A package
whose plan has not changed is not rebuilt. llvm, busybox and make are
rebuilt here by their stage-1 selves; run `cargo make pkgs` again and
everything rebuilds with the stage-2 tools. Packages that build each
other (llvm and python) use the locked product of the one asked for, so
`cargo make pkgs llvm` builds python first.

The generated script starts with `SRC` (the tarballs), `FILES`,
`SOURCES`, `VERSION`, `JOBS`, `LIVE` (the slot path the pack is used at,
`/oath/store/pkg/<name>/live`), `CC=cc` and friends, and `CPPFLAGS`,
`LDFLAGS` and `PKG_CONFIG_PATH` covering every library pack it builds
with. Library packs install under `$LIVE` so their `.pc` files work in
the next sandbox. Write the result to `/out`. A product must not name
its compiler's store path (`/oath/store/pkg/llvm/<hash>`), or it changes
with every `pkg:llvm`: strip debug info from anything built with `-g`
(python does).

`rust/` is the vendor's Rust release rather than a source build: its
tarballs are the pinned sources, and its plan gives the vendor's
binaries their own musl loader and `libgcc_s.so.1`. It builds the Oath
binaries, which are not a directory here: `cargo make build` (or
`cargo make pkgs oath`) packs this tree and every crate in `Cargo.lock`
into `pkg:oath-src` and builds `pkg:oath` with
`crates/oath-make/src/oath-pkg.sh`, linked by `pkg:llvm`'s clang against
its musl. That product follows the tree and is not in the lock.

`cargo make pkgs --list` shows every package, its licence and its pins.
`cargo make build` packs the image from the products in `lock.json`:
the profile's packages, and the kernel and its modules from
`pkg:linux` (`linux/`: the fragment and `patches/`).
