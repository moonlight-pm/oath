**Date:** 2026-09-05
**Status:** target (freeze)
**Implementation:** packing scripts + seed; canto apply generation 19
**Dogfood:** canto `/bin/cc` `/bin/rustc` `/bin/cargo` `/bin/cmake` `/bin/ninja`
  `/bin/pkg-config` linked. `cargo --version` 1.98.1. Default `cc` passes
  pkg:glibc interp+rpath. rustc loader path includes `pkg:git` libz.
**Gaps:** empty `.pc` farm; Sola `.cargo/config.toml` NixOS rpath is
  Oath-compat work, not this freeze. First guest `cargo build` of Sola
  kit ELFs **failed** (`alsa-sys` wants `alsa.pc`). Zig `cc` host link
  for rustc is `image/oath-cc-link.sh` + `zig-gnu-cc.sh` (not smoked
  to a finished Sola ELF).
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Guest toolchain (`pkg:cc` / `pkg:rustc` / `pkg:cmake` / `pkg:pkg-config`)

T24 inner loop on canto: build Oath, Thoxa, and Sola **on Oath**. No
Nix. No QEMU. Official upstream tarballs, relocated into the store,
catalog-owned. Same class as `pkg:grok` / `pkg:git` / `pkg:curl`.

---

## Locks this freeze owns

- **Four removable `pkg:*` objects.** `pkg:cc`, `pkg:rustc`,
  `pkg:cmake`, `pkg:pkg-config`. Bits under
  `/oath/store/pkg/<name>/`. `/bin` is the symlink farm. No new kind.
  No rustup. No self-update.
- **`pkg:cc` is the C toolchain.** Official Zig linux x86_64 tarball
  (static musl ELF). Product face is `cc` / `c++` / `gcc` / `g++` /
  `ar` / `ranlib` / `musl-cc` / `patchelf`. Default `cc` target is
  **glibc** (`x86_64-linux-gnu`) so Sola and the rustc host link.
  `/bin/musl-cc` is musl for Oath guest ELFs. patchelf is built with
  `zig c++` so later glibc ELFs can be relocated.
- **`pkg:rustc` is rustc + cargo + rust-std.** Official standalone
  `x86_64-unknown-linux-gnu` (glibc, `pkg:glibc` loader) plus
  `rust-std` for `x86_64-unknown-linux-musl`. rustc ≥ 1.85 (Sola
  edition 2024). Not musl-hosted rustc (that needs a musl ld.so we
  do not ship).
- **`pkg:cmake` includes ninja.** Kitware linux-x86_64 cmake plus
  ninja as the generator (`CMAKE_GENERATOR=Ninja`). No GNU make.
- **`pkg:pkg-config` is an empty farm.** `/bin/pkg-config` exists.
  `--exists` fails. Bundled C (aws-lc-sys) compiles with cc+cmake.
- **Seat env** (`host:local.env`): `CC`, `CXX`, `AR`,
  `CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/bin/musl-cc`,
  `CMAKE_GENERATOR=Ninja`. Wrappers also export these so a raw
  `/bin/cargo` works.
- **No Nix, no QEMU** as the way these bits land on canto. Fill is
  `image/pack-*.sh` (curl official URLs) then apply. `cargo make
  build` on a pack host may run the same scripts; it does not
  `nix-build` a compiler.

---

## Courage test

1. `test -x /bin/cc` and `readlink /bin/cc` is
   `/oath/store/pkg/cc/bin/cc`. `cc` compiles a glibc `int main`.
2. `test -x /bin/musl-cc`. `test -x /bin/patchelf`.
3. `rustc --version` and `cargo --version`. `rustc --print target-list`
   includes `x86_64-unknown-linux-musl`.
4. `cmake --version`. `ninja --version`.
5. `pkg-config --version` prints a version; `pkg-config --exists libc`
   is non-zero.
6. `oath get pkg:rustc` desired `present` is true; removable.

---

## Out

- Rebuilding River / wlroots (Zig project, not `zig cc`)
- A glibc `-dev` sysroot / `.pc` farm
- rustup, rustc self-update
- Guest image packing (`qemu-img`, kernel tree) — not the inner loop
- Splitting `pkg:sola`
