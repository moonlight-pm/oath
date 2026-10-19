# pkgs/

Every package that ships in Oath is built from source here (R7 in
[../docs/decisions.md](../docs/decisions.md)).

| File | What |
|------|------|
| `<name>/pkg.toml` | version, licence (SPDX), description, upstream sources pinned by sha256 or a `tree` of this repo, the packs it builds with (`build`) and needs at runtime (`run`) |
| `<name>/build.sh` | the build, run by `oath build` in the sandbox: no network, nothing from the build host, only the declared packs |
| `<name>/*` (other files and directories) | configs and patches; the plan sees them under `$FILES` |
| `<name>/<name>.plan` | generated: the exact plan that was built (source pack and every build pack pinned by hash) |
| `lock.json` | what each name resolves to: version, licence, how it was built, plan hash, product hash |
| `keys/<key>.asc` | one upstream's OpenPGP signing key, armored, with where it came from in its `Comment:` |
| `keys/sigstore.json` | Sigstore's trusted root (Fulcio CAs, Rekor keys), verbatim from `sigstore/root-signing` (`targets/trusted_root.json`) |
| `bootstrap.sh`, `bootstrap.nix` | stage 0 and stage 1 on the build host |

## Signatures

Every `[[source]]` either names its upstream's signature or says why it
has none:

```toml
[[source]]
url = "https://ftp.gnu.org/gnu/m4/m4-1.4.21.tar.xz"
sha256 = "f25c…"
sig = "https://ftp.gnu.org/gnu/m4/m4-1.4.21.tar.xz.sig"
key = "m4"                 # pkgs/keys/m4.asc
```

- `decompress = "xz"`: the signature is over the uncompressed file
  (kernel.org's `.tar.sign`).
- `sums = "<url>"`: the signature is over a `sha256sum` list, which
  must name the file with its `sha256` (CMake), or over a JSON release
  manifest whose `platforms.<p>` names it as `binary` with its
  `checksum` (Claude Code).
- `sigstore = "<url>"`, `identity = "<email or URI>"`,
  `issuer = "<OIDC issuer>"`: an upstream that signs only with
  Sigstore (Python). The bundle's certificate must chain to a Fulcio CA
  in `keys/sigstore.json` and name that identity and issuer, its key
  must have signed the pinned sha256, and Rekor's key must have signed
  the log entry (made while the certificate was valid) and the
  checkpoint its inclusion proof leads to.
- `unsigned = "<why>"`: no signature to check. A package-level
  `unsigned` covers all of its sources that have no `sig` (firmware).
- `tree = "apps/<dir>"` (package level): first-party source from this
  repository, not an upstream's. What git tracks or would add there
  goes into the source pack as `src/<dir>/`, so ignored build output
  stays out (a submodule, `forks/sola`, is listed by its own git); the
  manifest names the tree and the commit.
- `recipe = "<why>"` (package level): the licence does not allow
  redistribution, so the package is a recipe (R23): images and origins
  carry its plan, never its pack, and each machine fetches the one
  `[[source]]` itself and builds it (`oath build plan:<name>`). The
  source pack is that file alone (`src/<file>`, not executable), as a
  machine's fetch makes it; `build` is not prefixed with the base packs
  and may name only packs the base profile ships; there are no other
  files. A profile names `pkg:<name>` and `pkg:<name>-src` absent and
  `plan:<name>`; the image fills in their pins (`pkgs/claude-code`).
- `cargo_vendor = "<dir>"` (package level): the crates a Cargo
  workspace of this repository locks, crates.io and git alike, pinned
  by its `Cargo.lock`: `cargo vendor` on the build host writes them to
  `src/vendor/` with the source replacement in `src/vendor.toml`.
  Windows-only crates keep only their manifests (`pkg:sola-crates`).

Every fetch checks the sha256 pin, then the signature (cached under
`build/fetch/sig/`). A key file holds exactly the key that signs the
pinned release, exported minimal from the upstream's own published
key where there is one, else from keyserver.ubuntu.com. To bump a
version, change `url`, `sha256` and `sig` together. If the new release
is signed by a different key, the fetch fails and names that key. A
person adds the key to `keys/<key>.asc` from the upstream's published
list; that is the trust decision, and an agent does not make it alone.
A Sigstore identity is pinned the same way: a release signed by
someone else fails and names who signed it. `cargo make pkgs --list`
shows each package's keys or Sigstore identities and its unsigned
sources.

## Bootstrap

`cargo make bootstrap` (hours) builds the toolchain from the pinned
(and signature-checked) sources in `gcc/pkg.toml` and `glibc/pkg.toml`,
and the base tools from their own (R17, R18 in
[../docs/decisions.md](../docs/decisions.md)):

1. **Stage 0**, with the build host's gcc (`nix-shell bootstrap.nix`):
   a cross toolchain for `x86_64-oath-linux-gnu`: binutils, a C-only
   GCC, the Linux UAPI headers, glibc, then the whole GCC (C, C++,
   shared libgcc, libstdc++). It starts from an empty prefix every time,
   runs only on the build host and never ships.
2. **Stage 1**, with stage 0: `pkg:gcc` (GCC and binutils for
   `x86_64-pc-linux-gnu`), `pkg:glibc` (stage 0's glibc and headers) and
   the GNU tools: bash, coreutils, sed, grep, gawk, findutils, diffutils,
   patch, tar, gzip, xz, bzip2 and make. These are the base every plan
   builds with (`BASE` in `crates/oath-make/src/pkgs.rs`). The host's
   pkg-config sees nothing during stage 1, so no host library can be
   linked in.

GCC is patched (one define) to name `/lib/ld-linux-x86-64.so.2` as
every program's loader, and glibc installs it there and searches `/lib`;
there is no `/lib64`. `pkg:gcc` reads glibc's headers from
`/oath/store/pkg/glibc/live/include` and finds glibc's startup files and
libraries in `/lib`. After the bootstrap, `gcc/build.sh` and
`glibc/build.sh` rebuild them in the sandbox, and so does each base
tool's own plan.

## Stage 2

`cargo make pkgs [names]` builds packages in dependency order. For each:
fetch and verify the sources; put them (and the package's extra files)
in a `pkg:<name>-src` pack; write `<name>.plan` with the source pack, the
base packs and the packs `build` names, all pinned by hash from
`lock.json`; run `oath build` in the catalog at `.cache/oath`; record the
product in `lock.json`. A package whose plan has not changed is not
rebuilt. The base packs are rebuilt here by their stage-1 selves; run
`cargo make pkgs` again and everything rebuilds with the stage-2 tools.
Packages that build each other (glibc and python) use the locked
product of the one asked for, so `cargo make pkgs glibc` builds python
first. `cargo make pkgs --fetch [names]` only fetches and checks
sources.

The sandbox has a `/bin` and a `/lib` link farm of its packs. The
generated script starts with `SRC` (the tarballs), `FILES`, `SOURCES`,
`VERSION`, `JOBS`, `LIVE` (the slot path the pack is used at,
`/oath/store/pkg/<name>/live`), `CC=cc` and friends, and `CPPFLAGS`,
`LDFLAGS` and `PKG_CONFIG_PATH` covering every library pack it builds
with. Libraries build shared and install under `$LIVE` so their `.pc`
files work in the next sandbox; a package that loads one names it under
`run`, so machines get it too. After each build, every library the
product's programs load (`DT_NEEDED`) must be in its own `lib/`,
glibc's, or a `run` pack's (and what those run with); the build fails
naming the pack that has it. A base pack counts: zstd links liblzma, so
it runs with xz; C++ programs run with gcc (`libstdc++`, `libgcc_s`).
Only `<pack>/lib/pkgconfig` is on `PKG_CONFIG_PATH`, so a data-only
pack that installs its `.pc` in `share/pkgconfig` moves it
(wayland-protocols). There is no `/usr`: a Python script meson would run by its
`#!/usr/bin/env` line is made non-executable, so meson runs it with its
own python (libevdev); any other interpreter's `#!` line is rewritten to
its path (xkeyboard-config's perl). Python modules for a build are
packs with `lib/python/<module>` that the build puts on `PYTHONPATH`
(Mesa). Write the result to `/out`. A product
must not name its compiler's store path (`/oath/store/pkg/gcc/<hash>`),
or it changes with every `pkg:gcc`: strip debug info from anything built
with `-g` (python does).

`rust/` is the vendor's Rust release rather than a source build: its
tarballs are the pinned sources, and its plan points the vendor's
binaries at `/lib`'s loader (they name `/lib64/…`). It builds the Oath
binaries, which are not a directory here: `cargo make build` (or `cargo
make pkgs oath`) packs this tree and every crate in `Cargo.lock` into
`pkg:oath-src` and builds `pkg:oath` with
`crates/oath-make/src/oath-pkg.sh`: static, against glibc's `libc.a`,
linked by `pkg:gcc`. That product follows the tree and is not in the
lock. `kexec` is static too: the installer runs it on the machine being
replaced, whatever its libc.

`cargo make pkgs --list` shows every package, its licence and its pins.
`cargo make build` packs the image from the products in `lock.json`:
the profile's packages, and the kernel and its modules from
`pkg:linux` (`linux/`: the fragment and `patches/`).
