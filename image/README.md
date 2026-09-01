# Image bits

Borrowed **build-host** tools (kernel, busybox, qemu, musl cc) come from
`tools.nix`. They are not the runtime. Patched River is built from
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
