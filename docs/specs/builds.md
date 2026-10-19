# Builds

A plan is one hashed file that says how to turn pinned input packages
into one product package. `oath build` runs it in a sandbox with no
network and only the declared inputs, then hashes `/out` into the
store. The same plan bytes and the same input hashes must give the
same product hash. Plans are how Oath makes native packages.

**Status:** built for the headless base and the desk's packages
([desk.md](desk.md), 2026-10-18). The plan kind, `oath fmt`,
`oath build` (pinned and `--file`), the sandbox and the native
toolchain are built; every package in the image is a plan product: the
C packages, the toolchain (`pkg:gcc`, `pkg:glibc`) and the kernel
(`pkg:linux`) from `pkgs/` via
`cargo make pkgs`, and the Oath binaries (`pkg:oath`) from this tree,
built by the vendor's Rust release (`pkg:rust`), and sources are checked
against their upstreams' OpenPGP or Sigstore signatures. Products reach
the store whole or not at all, and every build logs to
`/oath/log/plan/<name>.log`.

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
    {"id": "pkg:bash", "hash": "sha256-…"},
    {"id": "pkg:gcc", "hash": "sha256-…"},
    {"id": "pkg:glibc", "hash": "sha256-…"}
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
   only the build, and runs as `home` when the builder entered as root
   (it drops to `home` after mounting `/proc`, which needs root).
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

GCC 16.2.0 and binutils 2.47 on glibc 2.44 are the toolchain and the C
library (R17 in [decisions](../decisions.md)); the base tools are GNU's
(R18). Clang/LLVM and musl (R11) left on 2026-10-17. Zig is not used
(R9).

- **Stage 0** (`cargo make bootstrap`, `pkgs/bootstrap.sh`): a cross
  toolchain for `x86_64-oath-linux-gnu` built by the build host's gcc:
  binutils, a C-only GCC, the Linux UAPI headers (from the pinned kernel
  tarball, `make headers`), glibc, then the whole GCC (C, C++, shared
  libgcc, libstdc++). GMP, MPFR and MPC build inside GCC's tree. It
  starts from an empty prefix, runs only on the build host and never
  ships.
- **Stage 1**, built by stage 0 for `x86_64-pc-linux-gnu`: `pkg:gcc`
  (GCC and binutils), `pkg:glibc` (stage 0's) and the GNU tools bash,
  coreutils, sed, grep, gawk, findutils, diffutils, patch, tar, gzip,
  xz, bzip2 and make: the base every plan builds with. The host's
  pkg-config sees nothing during stage 1, so no host library is linked
  in (binutils once linked the host's libzstd that way).
- **The loader.** GCC is patched (`GLIBC_DYNAMIC_LINKER64`) to name
  `/lib/ld-linux-x86-64.so.2`; glibc installs it there
  (`slibdir`/`rtlddir` `/lib`) and searches `/lib`. There is no
  `/lib64` and no RPATH: the sandbox and every machine have a `/lib`
  link farm of their packs' `lib/` entries. `pkg:gcc` reads glibc's
  headers from `/oath/store/pkg/glibc/live/include`.
- **Sources** (`cargo make pkgs` and `bootstrap`, on the build host):
  every source is pinned by sha256 and, where its upstream signs, by an
  OpenPGP signature checked on every fetch against one key per upstream
  in `pkgs/keys/<key>.asc` (57 keys). The key is the trust pin: a
  release signed by another key fails until a person adds that key.
  kernel.org signs the uncompressed tar (`decompress = "xz"`). Python
  signs only with Sigstore: its bundle is checked against Sigstore's
  trusted root (`pkgs/keys/sigstore.json`) and the identity pinned in
  `pkg.toml` (`hugo@python.org` via GitHub): Fulcio chain, identity and
  issuer, the signature over the pinned sha256, Rekor's signed entry
  timestamp, the entry's inclusion proof up to a checkpoint Rekor
  signed, and the certificate's validity at the time it was logged. The
  rest say why not (`unsigned`): Perl's `.tar.xz` bundle signs its
  `.tar.gz` (upstream error); Linus's rc tarball and the firmware blobs
  are cgit output; lzo, mbedTLS, Ninja, Vim, patchelf, pkgconf and the
  CA bundle publish no signature; crates are pinned by `Cargo.lock`.
  Both verifiers are `oath-make`'s own (OpenPGP v4 with RSA, DSA and
  Ed25519 over SHA-1 and SHA-2; Sigstore bundles with ECDSA, Rekor v1
  `hashedrekord`); neither checks expiry or revocation (glibc 2.44 is
  signed by a subkey past its stated expiry).
- **Stage 2** (`cargo make pkgs`): every package in `pkgs/` is a plan
  pinned to its source pack (`pkg:<name>-src`), the base packs and its
  own build packs, with what each of those runs with (bison runs m4),
  all from `pkgs/lock.json`; the lock records each product's run needs
  as it was built. It runs in the same user-namespace `oath build`
  sandbox against the build catalog at `~/.local/share/oath`; GNU tar there
  does not restore owners (`TAR_OPTIONS=--no-same-owner`). The host's
  compilers are not inputs. How a package is written: `pkgs/README.md`.
- **The toolchain by plan.** `pkgs/glibc/build.sh` builds glibc and the
  UAPI headers with `pkg:gcc`; `pkgs/gcc/build.sh` builds binutils and
  GCC with the previous `pkg:gcc`, against the GMP, MPFR and MPC
  packages. gawk 5.4.1 is built with MPFR: without it, it miscompiles
  the awk scripts that generate GCC 16.2's option tables.
- **Build tools** (plan products, not in the image): Ninja 1.13.2 (its
  own bootstrap), Meson 1.12.1 and CPython 3.15.0 (stdlib extensions as
  shared modules; no `_ctypes`, `_decimal`, `_ssl` or `_sqlite3`: no
  libffi, libmpdec, OpenSSL or SQLite pack). A product must not name its
  compiler's store path: CPython's `-g` DWARF did, so it is stripped of
  debug info. For the kernel: GNU m4 1.4.21, flex 2.6.4, Bison 3.8.2,
  bc 1.08.2, Perl 5.44.0 (every core XS module linked in) and libelf
  from elfutils 0.196.
- **The kernel** (`pkgs/linux`): kernel.org 7.3-rc1, `patches/` and
  `linux.fragment` over `defconfig`, built by `pkg:gcc`. Build user,
  host, time and counter are fixed. The plan writes `modules.dep`,
  `modules.alias` and `modules.softdep` from each module's `.modinfo`
  with `objcopy` (no kmod). The signed wireless regdb and the trusted
  keyring are off: nothing else wanted the keyring, and its
  `extract-cert` needs OpenSSL. defconfig's `WERROR` is off: a compiler
  newer than the rc may warn where the rc did not. `patches/` also
  carries a kbuild fix for a parallel-build race in 7.3-rc1 (the
  asm-headers sweep deleted another rule's temp file).
- **The base on machines** (plan products): glibc, bash, coreutils,
  sed, grep, gawk (with GMP and MPFR), findutils, diffutils, tar, gzip,
  xz, bzip2, procps-ng, less, Vim (ncurses), iproute2, iputils, dhcpcd,
  chrony, util-linux, btrfs-progs (zlib, zstd, LZO), dropbear, OpenSSH's
  client tools (no OpenSSL), mbedTLS, curl (pinned CA bundle),
  BIND 9.20's dig, host and nslookup (no named, DoH or IDN) with
  OpenSSL 4.0, libuv, liburcu and libcap, dosfstools, kexec-tools and linux-firmware blobs pinned one by one.
  Libraries are shared; a package names what it loads under `run`.
- **Rust** (`pkgs/rust`). The vendor's release, pinned by sha256 (R7):
  rustc and cargo 1.99.1 built for a glibc host, and the standard
  library for `x86_64-unknown-linux-gnu` and `x86_64-unknown-uefi`.
  `patchelf` (a build-only pack) points every executable's interpreter
  at `/lib/ld-linux-x86-64.so.2` (they name `/lib64/…`); `libgcc_s`
  comes from `pkg:gcc` through `/lib`. The plan checks that rustc runs
  and can still report an error (it does so by unwinding).
- **The Oath binaries** (`pkg:oath`). `cargo make build` packs this tree
  (the workspace, `brand/logo-white.png`, and every crates.io crate in
  `Cargo.lock`, fetched and checked against the lock's sha256) into
  `pkg:oath-src`, and builds it with `pkg:rust` and the base packs in
  the sandbox: the crates are a vendored source, cargo runs `--frozen`.
  Product: `bin/oath`, `lib/oath/{init,serial-login,screen-login,sudo}`,
  `boot/BOOTX64.EFI` and a `components.json` listing every crate in the
  binaries (`pkg:cargo/…`, from `cargo tree`). The product follows the
  tree, so it is not in `pkgs/lock.json`; an unchanged tree is not
  rebuilt (`build/pkgs/oath.json`). The Linux binaries are static PIE
  against glibc's `libc.a` (`crt-static` for that target only, so proc
  macros stay shared), linked by `pkg:gcc`; the plan fails if one names
  an interpreter. oath-efi links with `/Brepro`, so its PE timestamp is
  a hash. The build host's Rust compiles only `oath-make`.

- **Recipes** (R23, built 2026-10-19). A package whose licence does
  not allow redistribution (`recipe` in its `pkg.toml`) is built on the
  machine: its plan pins one source pack, the vendor's file alone as
  `src/<file>`, and only packs the base profile ships, so a fresh
  machine has every input but the file, which pre-build fetches by
  `url` and `file` and checks against the pin. `cargo make pkgs`
  builds it on the build host too, for the product hash the seed pins:
  the machine's build must make that tree. `pkg:claude-code` is
  Anthropic's binary (stable channel), checked on the build host
  against the manifest Anthropic signs; its plan writes
  `/lib/ld-linux-x86-64.so.2` over the ELF's `/lib64/…` name in place
  (shorter, NUL-padded, so nothing moves: Bun's bundle is an ELF
  section), adds `bin/claude` with `DISABLE_UPDATES=1`, and runs
  `--version` in the sandbox. On novus it builds in seconds; on a
  machine the fetch is about 240 MB.

## Products and logs (built)

- The script writes `/out` in a scratch directory. The product is
  copied beside its slot (`store/pkg/.<name>.<hash>.part`), hashed
  again and renamed into place, so a realization is whole or absent;
  one that exists is verified, never written into. Every store writer
  (install, fetch, promote, the host cache) goes through the same step.
- The script's stdout and stderr go to `log/plan/<name>.log` in the
  catalog (`/oath/log/plan/<name>.log`; `~/.local/share/oath/log/plan/` for
  `cargo make pkgs`), headed by the plan hash and time and ended by the
  product or the failure. The build before is `<name>.log.1`.
  `oath build` prints the log's path, a failed build the log's last
  lines, and `oath logs plan:<name>` reads it.

## Target

- **The desk's session.** Its packages are built ([desk.md](desk.md));
  audio, fonts and Sola as plans remain (phase 8).

## Out

A recipe language or DSL, network inside the sandbox, multi-product
plans, automatic `present`, and treating `built_from` as attestation.
