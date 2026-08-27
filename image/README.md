# Image bits

Borrowed **build-host** tools (kernel, busybox, qemu, musl cc) come from
`tools.nix`. They are not the runtime.

Host orchestration is Rust:

```
cargo run -p oath-make -- build
cargo run -p oath-make -- probe
cargo run -p oath-make -- run
```
