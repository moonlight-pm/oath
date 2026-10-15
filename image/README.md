# image/

Build-host inputs to `cargo make` that never ship. Everything that
ships, the kernel included (`pkg:linux`), is a package in
[../pkgs/](../pkgs/README.md).

| File | What |
|------|------|
| `host.nix` | QEMU and OVMF for running images on the build host. Never ships. |

`cargo make build` takes the kernel and every userspace package from
`pkgs/lock.json`, and refuses a kernel whose bzImage release, module
directory and module vermagic disagree.
