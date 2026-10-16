# Builds

A plan is one hashed file that says how to turn pinned input packages
into one product package. `oath build` runs it in a sandbox with no
network and only the declared inputs, then hashes `/out` into the
store. The same plan bytes and the same input hashes must give the
same product hash. Plans are how Oath makes native packages.

**Status:** partly built. The plan kind, `oath fmt`,
`oath build` (pinned and `--file`), the sandbox and the native
toolchain are built; every package in the image is a plan product: the
C packages, `pkg:llvm` and the kernel (`pkg:linux`) from `pkgs/` via
`cargo make pkgs`, and the Oath binaries (`pkg:oath`) from this tree,
built by the vendor's Rust release (`pkg:rust`). Still target: upstream
signature checks, immutable outputs, build logs.

Replaces: T43, and the toolchain parts of T35.

## Plan kind (built)

- Catalog id `plan:<name>`. Store
  `/oath/store/plan/<name>/<hash>/plan.plan`. The realization id is
  SHA-256 of the file's bytes.
- One plan, one product: `name`, the slot, and `produces: "pkg:<name>"`
  agree, or the plan is refused.
- No anonymous blobs. Every input is a named package at a pin. A source
  tarball is a package such as `pkg:hello-src` with `url` and `hash`.

## File format (built)

```text
#!oath-plan
{
  "name": "hello",
  "produces": "pkg:hello",
  "build_needs": [
    {"id": "pkg:busybox", "hash": "sha256-…"},
    {"id": "pkg:llvm", "hash": "sha256-…"}
  ],
  "run_needs": []
}
---
#!/bin/sh
set -eu
…
```

The JSON head is pretty-printed with two-space indent, keys in exactly
that order, needs sorted by id. The script starts with `#!/bin/sh` and
`set -eu`. `oath fmt` writes the only legal form; `oath build` and
`oath set plan:<name> --from-file` refuse anything else rather than
rewrite it before hashing.

`build_needs` are what the sandbox sees. `run_needs` become the
product's `pkg.needs`.

## `oath build` (built)

1. Lint the plan: the pin (`oath build plan:<name>`) or a file
   (`oath build --file ./x.plan`, which does not file the plan).
2. Pre-build: every `build_needs` package must be in the store at its
   hash. Missing ones with a `url` are fetched and verified. This is
   the only step that uses the network.
3. Sandbox: new user, mount, network, IPC, UTS and PID namespaces; a
   chroot holding only the input packages and an empty `/out`;
   `PATH=/bin`; the script is PID 1 there, with a `/proc` that shows
   only the build, and runs as `home` when the builder entered as root.
4. Hash `/out` and write `/oath/store/pkg/<name>/<product>/`. Build
   never sets `present` and never links `/bin`; that is still
   `oath set` plus `oath apply`.
5. Record `product` and `inputs` on the plan's actual.

A rebuild of the same plan and inputs that yields a different product
hash is a security error: nothing is recorded, the live pin does not
move, and both trees stay.

The skip-if-built check is keyed on the plan hash and the input set,
so an edited plan always rebuilds.

The sandbox does not see `/nix/store`; only the crate's unit tests opt
in (`OATH_SANDBOX_NIX_STORE`).

## Native toolchain (built)

Clang/LLVM 23.1.3 (clang, lld, compiler-rt, libunwind, libc++) on musl
1.2.6 is the system toolchain (R11 in [decisions](../decisions.md)).
Zig is not used (R9).

- **Stage 0** (`cargo make bootstrap`, `pkgs/bootstrap.sh`): clang and
  lld targeting `x86_64-unknown-linux-musl`, plus a sysroot (Linux UAPI
  headers from the pinned kernel tarball, musl, compiler-rt builtins,
  libunwind, libc++abi, libc++), built by the build host's gcc. It
  runs only on the build host and never ships.
- **Stage 1**, built by stage 0: a static musl `pkg:llvm` (clang, lld,
  the LLVM binutils, its own sysroot; `bin/clang.cfg` points at that
  sysroot and links statically by default), static busybox and make.
- **Stage 2** (`cargo make pkgs`): every package in `pkgs/` is a plan
  pinned to its source pack (`pkg:<name>-src`) and to the build packs in
  `pkgs/lock.json`, built by the same `oath build` in a user-namespace
  sandbox against the build catalog at `.cache/oath`. The host's
  compilers are not inputs. How a package is written: `pkgs/README.md`.
- **`pkg:llvm` by plan.** `pkgs/llvm/build.sh` is stage 1 in the
  sandbox: the previous `pkg:llvm` builds a fresh sysroot (UAPI headers
  through `make headers`, musl, compiler-rt, libunwind, libc++abi,
  libc++) and then the tools. The bootstrap pack's binaries carry stage
  0's git revision in `.comment` and build-host paths in libc++abi's
  assertion strings; the plan's carry only sandbox paths.
- **Build tools** (plan products, not in the image): CMake 4.4.4
  (bundled third-party libraries, curl without TLS), Ninja 1.13.2 and
  CPython 3.15.0, one static binary with every stdlib extension linked
  in (no `_ctypes`, `_decimal` or `_ssl`: no libffi, libmpdec or
  OpenSSL pack). A product must not name its compiler's store path:
  CPython's `-g` DWARF did, so its binary is stripped of debug info.
  For the kernel: GNU m4 1.4.21, flex 2.6.4, Bison 3.8.2, Perl 5.44.0
  (static, every core XS module linked in; it generates the OID
  registry) and libelf from elfutils 0.196 (only libelf: the rest needs
  argp, fts and obstack, which musl lacks).
- **The kernel** (`pkgs/linux`): kernel.org 7.3-rc1, `patches/` and
  `linux.fragment` over `defconfig`, `make LLVM=1` with `pkg:llvm`;
  host tools (objtool, modpost) link statically. Build user, host,
  time and counter are fixed. busybox's `depmod` is the small one, so
  the plan writes `modules.dep`, `modules.alias` and `modules.softdep`
  from each module's `.modinfo` with `llvm-objcopy`. The signed
  wireless regdb and the trusted keyring are off: nothing else wanted
  the keyring, and its `extract-cert` needs OpenSSL. defconfig's
  `WERROR` is off: a compiler newer than the rc warns where the rc did
  not (amdgpu's DML stack frames). `patches/` also
  carries a kbuild fix for a parallel-build race in 7.3-rc1 (the
  asm-headers sweep deleted another rule's temp file).
- **Fixed point.** Rebuilding stage 2 with stage-2 tools gives
  bit-identical products: `pkg:llvm` rebuilt by itself is identical
  (2026-10-15), and a second `cargo make pkgs` rebuilds nothing. The
  kernel and its build tools rebuild identically too (2026-10-15), and
  so do `pkg:rust`, `patchelf` and `pkg:oath` (2026-10-16).
  Configure scripts that record time, host or user (perl's, the
  kernel's) get fixed values.
- **The headless base** (15 plan products): busybox, make, pkgconf,
  zlib, zstd, lzo, util-linux, btrfs-progs, dropbear, OpenSSH (no
  OpenSSL), mbedTLS, curl (mbedTLS, pinned CA bundle), dosfstools,
  kexec-tools and linux-firmware blobs pinned one by one.
- **Rust** (`pkgs/rust`). The vendor's release, pinned by sha256 (R7):
  rustc and cargo 1.99.1 built for a musl host, and the standard library
  for `x86_64-unknown-linux-musl` and `x86_64-unknown-uefi`. Those
  binaries are dynamically linked against musl and libgcc_s, so the
  pack carries its own: musl 1.2.6's shared libc (the loader) and LLVM's
  libunwind as `libgcc_s.so.1`, with compiler-rt's `__popcountdi2` and
  upstream's no-op frame-registration API (gcc's crtbegin in the non-PIE
  `rust-lld` calls it). `patchelf` (a build-only pack) points every
  executable's interpreter at `/oath/store/pkg/rust/live/lib`, and the
  loader's `etc/ld-musl-x86_64.path` searches only the pack. The plan
  fails if any vendor binary imports a symbol the pack does not define,
  and checks that rustc can still report an error (it does so by
  unwinding). The `.tar.gz` tarballs are pinned: rustc's `.tar.xz` uses
  a 128 MiB dictionary and busybox's unxz stops at 64 MiB.
- **The Oath binaries** (`pkg:oath`). `cargo make build` packs this tree
  (the workspace, `brand/logo-white.png`, and every crates.io crate in
  `Cargo.lock`, fetched and checked against the lock's sha256) into
  `pkg:oath-src`, and builds it with `pkg:rust`, `pkg:llvm` and busybox
  in the sandbox: the crates are a vendored source, cargo runs `--frozen`.
  Product: `bin/oath`, `lib/oath/{init,serial-login,sudo}`,
  `boot/BOOTX64.EFI` and a `components.json` listing every crate in the
  binaries (`pkg:cargo/…`, from `cargo tree`). The product follows the
  tree, so it is not in `pkgs/lock.json`; an unchanged tree is not
  rebuilt (`build/pkgs/oath.json`). It rebuilds bit-identically
  (oath-efi links with `/Brepro`, so its PE timestamp is a hash). The
  musl binaries are static PIE, linked by `pkg:llvm`'s clang (without
  its `clang.cfg`, whose `-static` would break proc macros) with
  `-C link-self-contained=no`: `pkg:llvm`'s musl 1.2.6, libunwind and
  startup files, as every C package. Rust's self-contained musl was
  compiled by gcc, so the plan fails if a binary's `.comment` names
  GCC. Build scripts still link that musl (cargo gives them none of the
  target's rustflags); they run only in the sandbox. The build host's
  Rust compiles only `oath-make`.
- **Desk later.** Mesa, Wayland, a compositor and the rest come after
  the base, as plans (phase 8).

Target: upstream signature checks on top of the sha256 pins.

## Target fixes

- Plan outputs are immutable; a build never writes into a tree that
  already exists.
- Builds log to `/oath/log` and `oath build` prints where.

## Out

A recipe language or DSL, network inside the sandbox, multi-product
plans, automatic `present`, and treating `built_from` as attestation.
