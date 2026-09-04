# Image bits

Borrowed **build-host** tools (kernel, busybox, qemu, musl cc) come from
`tools.nix`. They are not the runtime. Packing emits store trees
(`bin/` plus whatever the payload needs). There is no recipe language;
how you build that tree is the packer’s. Target pack identity (content
hash, hash-in-path, pin) is T32
([docs/specs/2026-09-03-pkg-pack-identity.md](../docs/specs/2026-09-03-pkg-pack-identity.md))
— not implemented. Patched River is built from
`forks/river` + `forks/wlroots` (Sola nixpkgs pin) and relocated into
`pkg:glibc` + `pkg:river` (libudev-zero, no udevd) + `pkg:sola`
(session stack including sola-session, sola-terminal + tmux, and sola-browser + CEF; host `cargo build --release` from `forks/sola`).

Host orchestration is Rust:

```
cargo make build
cargo make probe
cargo make run            # --build packs first (also up / start)
cargo make up
cargo make start && cargo make ssh && cargo make stop
```
