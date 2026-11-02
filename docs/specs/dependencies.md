# Reducing dependencies

R57's bias, tracked: every dependency is code nobody here read. Pins, age limits
and review ([intake.md](intake.md)) reduce that risk; not having it removes it. Order
of preference: what Oath ships, our own small code in `apps/`, a compiled signed
project with few dependencies, a large tree last.

Numbers are from 2026-11-02. A number here goes stale: "Refresh" below remakes them.
Update a row when its status moves, and move it to "Done" when it lands.

## Where we stand

| What | Count |
|------|-------|
| Packages with sources | 168 |
| Compared with an upstream tag or its own files (R54) | 158; the other 10 say why not |
| Pinned Go modules, distinct | 347 in Podman, `gh`, Blocky and restic; 235 once Podman goes |
| Pinned npm packages | 32, all `cf`'s |
| Crates in `Cargo.lock` | 174; `oath` links 64 on Linux, `oath-init` 58, `oath-make` 123 |
| Tarball-only files that no repository vouches for | 5,036, plus 525,587 in CEF's |

## Candidates, largest first

| Package | Tree | What could replace it | Would remove | Cost | Status |
|---------|------|-----------------------|--------------|------|--------|
| `pkg:podman` | 157 Go modules; 51 MB; with conmon, glib | `crun`, `pasta` and our own glue, one uid (R53) | All 157, and conmon | Small program; image builds are not covered | **Decided, building** (R59) |
| `pkg:gh` | 160 Go modules; 39 MB | `curl` on GitHub's API, or nothing | All 160 | Agents lose its conveniences | Open: who needs it? |
| `pkg:cf` | 32 npm packages; 203 MB, on `pkg:node` (205 MB, an hour to build) | `curl` on Cloudflare's API | All 32, and Node with them: nothing else runs on it | `cf dev` and Workers' local runtime go | Open: who needs it? |
| `pkg:blocky` | 80 Go modules; AGPL (parcour); unsigned | Unbound (signed C, needs OpenSSL and expat) with a generated blocklist | All 80 | Packaging, and a small list generator | Open: ask Wicket's session |
| `pkg:restic` | 37 modules, after the patch (86 before) | Drop S3 too | 12 more | S3 repositories go through rclone | Open: does anyone use S3? |
| `pkg:oath`'s crates | `oath` 64, `oath-init` 58 | See below | Up to half | Our own code | Later |
| `pkg:oath-make` (host tool) | 123 crates | See below | About 30 | Our own code | Later |

Needed and not yet here: `cloudflared` links 67 Go modules, builds from source, and
its tag is unsigned. Nothing else speaks Cloudflare's tunnel protocol, so it is not a
candidate. It needs `go-modules.sh` to follow `replace`, and OSV flags its forked
`quic-go`, unread. Not packaged until something asks.

### Our own crates

What each would leave, on Linux, by `cargo tree`:

| Without | `oath-core` | `oath` | `oath-init` | `oath-make` |
|---------|-------------|--------|-------------|-------------|
| Today | 51 | 64 | 58 | 123 |
| `croner` (a cron parser: our own is small) | 38 | 53 | 45 | 112 |
| `schemars` (JSON Schema by derive) | 45 | 58 | 52 | 117 |
| `clap` (argument parsing) | 51 | 53 | 58 | 112 |
| `croner`, `schemars` | 32 | 47 | 39 | 106 |
| `croner`, `schemars`, `clap` | 32 | 34 | 39 | 93 |
| and `chrono`, `thiserror` | 27 | 29 | 34 | 89 |
| and `nix` (`libc` directly) | 27 | 27 | 32 | 89 |

`ed25519-dalek`, `argon2` and `sha2` stay: nobody here writes those. `serde` stays.
`oath-make`'s remainder is mostly `rsa`, `dsa`, `p256`, `p384` and `x509-cert`, for
checking signatures, and `toml`, `tempfile`, `flate2`, `regex`.

### Build steps nobody can check

66 packages' tarballs add autotools' output (`configure`, `Makefile.in`), which no
repository vouches for and the build runs: the kind of file xz's backdoor used.
Building from the tree instead removes it.

- 22 trees also build with meson or CMake: curl, fontconfig, fribidi, git, libevdev,
  libevent, libfontenc, libsndfile, libuv, libxau, libxcomposite, libxdamage,
  libxfixes, libxrandr, libxrender, libxxf86vm, patchelf, pkgconf, util-linux, xkbcomp,
  xorgproto, xz. Those in the base (curl, util-linux, xz, pkgconf) rebuild everything.
- 44 are autotools only, including coreutils, grep, sed, tar and most of GNU. They
  would need autoconf, automake and libtool as packages first.

## Done

| What | Before | After | How |
|------|--------|-------|-----|
| HTTP routing | `pkg:caddy`, 148 modules, 49 MB | `pkg:route`: 0 modules, 6 MB, 370 lines | Our own, in `apps/route` (R59) |
| `pkg:caddy` | 148 modules, 49 MB | gone | Behind a tunnel, TLS ends at Cloudflare (R58) |
| `pkg:netavark`, `pkg:protobuf`, Abseil | 257 crates and a C++ build | a 12-line script | Rootless containers use pasta |
| `pkg:lzo` | no public history | gone | `btrfs restore` loses LZO extents; the kernel keeps them |
| `pkg:restic` | 86 modules, gRPC, OpenTelemetry | 37 | Patched out Azure, Backblaze, Google Cloud, Swift |
| `pkg:libxml2` | autotools' output | none | Built from the signed tree with meson |
| `pkg:qemu` | 66,682 ROM source files in the build | 0 | Deleted first; the product did not change |
| `pkg:sqlite` | a generated bundle | the tree | Built from the tag |
| `pkg:certs` | curl.se's generated list | Mozilla's `certdata.txt` | Converted here (R50) |

## Looked at and kept

| What | Why it stayed |
|------|---------------|
| GNU bc | No repository, but signed, and only computes one kernel header. The other bc's author marks its latest release untested. |
| GMP | Mercurial, whose archives refuse automated fetches; signed; GCC needs it. |
| Rust, Codex, Claude Code, Grok, NVIDIA, Steam | The vendor's build is what they are. |
| Iosevka | Built font files. |
| CEF | 288,065 files match Chromium's tag; 368,476 come from its submodules. It is what Sola needs. |
| A smaller Caddy | Only saved 14 of 146 modules. Moot now. |
| nginx, HAProxy | Zero Go modules, but large C codebases, for a job 200 lines do. nginx is signed, and the fallback if caching or file serving is ever wanted. |

## Refresh

```sh
# A package's pinned modules
grep -c '^module ' pkgs/intake/<name>.txt
# Our crates, on Linux
cargo tree -p oath -e normal --prefix none | sed 's/ (\*)//; s/ (proc-macro)//' | sort -u | wc -l
# What no repository vouches for
cat pkgs/tags/*.txt | grep -c '^adds '
```
