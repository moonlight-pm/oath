# image/

Inputs to `cargo make build` that are not packages (packages are in
[../pkgs/](../pkgs/README.md)).

| File | What |
|------|------|
| `build-linux.sh` | The one kernel: kernel.org tarball pinned by sha256, `linux-patches/`, `linux.fragment`. Output `build/linux/{vmlinuz,modules}`. Run with `nix-shell linux-build.nix --run 'sh image/build-linux.sh'`. |
| `linux.fragment` | Config merged over `defconfig`. |
| `linux-patches/` | Driver fixes (SI DPM on Pitcairn), gated by chip family. |
| `linux-build.nix` | Host compilers for the kernel build. Build-time only. |
| `host.nix` | QEMU and OVMF for running images on the build host. Never ships. |
| `fetch-url.sh` | Download with an optional sha256 check (build-linux.sh). |

`cargo make build` refuses a kernel whose bzImage release, module
directory and module vermagic disagree, and takes every userspace
package from `pkgs/lock.json`.
