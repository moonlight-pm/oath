# Builds

A plan is one hashed file that says how to turn pinned input packages
into one product package. `oath build` runs it in a sandbox with no
network and only the declared inputs, then hashes `/out` into the
store. The same plan bytes and the same input hashes must give the
same product hash. Plans are how Oath makes native packages.

**Status:** partly built. The plan kind, `oath fmt`, `oath build`
(pinned and `--file`) and the sandbox are built. Native packages built
by plans are the target; today only canaries (`apps/hello.plan`,
`apps/reloc.plan`) use it.

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
    {"id": "pkg:cc", "hash": "sha256-…"}
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

Known defect: the skip-if-built check compares only the input set, not
the plan hash, so editing a filed plan without changing its inputs
reuses the old product.

## Native toolchain (target)

The direction, from D7:

- **Stage 0.** `pkg:cc` is the Zig release from ziglang.org, pinned by
  sha256. It is a static binary. Its `cc`, `c++` and `musl-cc` wrappers
  all target `x86_64-linux-musl`. With busybox it is enough to build C
  from source inside the sandbox.
- **Build where.** The same `oath build` runs on the build host (under
  a user namespace, against a scratch catalog) to produce the image's
  packages, and on Oath itself to rebuild them. The host's own
  compilers are not inputs.
- **First native set (the headless base).** musl, busybox, btrfs-progs
  and its libraries (util-linux libuuid and libblkid, zlib, lzo, zstd),
  dropbear, curl with a CA bundle, dosfstools, a GPT tool, and
  linux-firmware as a vendor release. The kernel is already built from
  a pinned kernel.org tarball (boot-and-install.md).
- **Rust.** The vendor's `x86_64-unknown-linux-musl` toolchain release,
  pinned. The Oath crates are static musl binaries already.
- **Desk later.** Mesa, Wayland, a compositor and the rest come after
  the base, as plans.

Open: whether the long-term system C compiler stays Zig (clang) or
becomes GCC built from source. Record the choice in decisions when it
is made.

## Target fixes

- Cache key is `(plan hash, input-set hash)`.
- Plan outputs are immutable; a build never writes into a tree that
  already exists.
- Builds log to `/oath/log` and `oath build` prints where.

## Out

A recipe language or DSL, network inside the sandbox, multi-product
plans, automatic `present`, and treating `built_from` as attestation.
