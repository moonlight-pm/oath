# Image bits

Borrowed **build-host** tools (kernel, busybox, qemu, musl cc) come from
`tools.nix`. They are not the runtime.

Host orchestration is Rust:

```
cargo make build
cargo make probe
cargo make run
```
