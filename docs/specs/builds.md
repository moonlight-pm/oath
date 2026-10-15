# Builds

A plan is one hashed file that says how to turn pinned input packages
into one product package. `oath build` runs it in a sandbox with no
network and only the declared inputs, then hashes `/out` into the
store. The same plan bytes and the same input hashes must give the
same product hash. Plans are how Oath makes native packages.

**Status:** partly built. The plan kind, `oath fmt`,
`oath build` (pinned and `--file`), the sandbox and the native
toolchain are built; every C package in the image is a plan product
(`pkgs/`, via `cargo make pkgs`), and so is `pkg:llvm`. Still target:
the kernel and the Rust toolchain as packs built or pinned by Oath,
immutable outputs, build logs.

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
3. Sandbox: new user, mount, network, IPC and UTS namespaces; a chroot
   holding only the input packages and an empty `/out`; `PATH=/bin`;
   the script runs as `home` when the builder entered as root.
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
- **Fixed point.** Rebuilding stage 2 with stage-2 tools gives
  bit-identical products: `pkg:llvm` rebuilt by itself is identical
  (2026-10-15), and a second `cargo make pkgs` rebuilds nothing.
- **The headless base** (15 plan products): busybox, make, pkgconf,
  zlib, zstd, lzo, util-linux, btrfs-progs, dropbear, OpenSSH (no
  OpenSSL), mbedTLS, curl (mbedTLS, pinned CA bundle), dosfstools,
  kexec-tools and linux-firmware blobs pinned one by one.
- **Rust.** The Oath crates are static musl binaries linked by
  `rust-lld` with the Rust musl target's self-contained crt and libc
  (Rust's bundled musl, not `pkg:llvm`'s 1.2.6). The Rust toolchain
  itself is still the build host's; the vendor's pinned release as a
  pack is target.
- **Desk later.** Mesa, Wayland, a compositor and the rest come after
  the base, as plans (phase 8).

Target: the kernel built by a plan with `pkg:llvm` instead of the
host's gcc; upstream signature checks on top of the sha256 pins.

## Target fixes

- Plan outputs are immutable; a build never writes into a tree that
  already exists.
- Builds log to `/oath/log` and `oath build` prints where.

## Out

A recipe language or DSL, network inside the sandbox, multi-product
plans, automatic `present`, and treating `built_from` as attestation.
