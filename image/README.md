# image/

Inputs to `cargo make build` that are not Rust.

| File | What |
|------|------|
| `build-linux.sh` | The one kernel: kernel.org tarball pinned by sha256, `linux-patches/`, `linux.fragment`. Output `build/linux/{vmlinuz,modules,firmware}`. Run with `nix-shell linux-build.nix --run 'sh image/build-linux.sh'`. |
| `linux.fragment` | Config merged over `defconfig`. |
| `linux-patches/` | Driver fixes (SI DPM on Pitcairn), gated by chip family. |
| `linux-build.nix` | Host compilers for the kernel build. Build-time only. |
| `pack-cc.sh` | `pkg:cc`: the Zig release (sha256-pinned) plus patchelf built from source; every wrapper targets musl. `dev` profile only. |
| `write-components.sh` | Writes a pack's `components.json` (upstream name, version, purl). |
| `fetch-url.sh` | Download with an optional sha256 check. |
| `tools.nix` | **Temporary.** Busybox, btrfs-progs, dropbear, OpenSSH client bits, curl, CA bundle, sgdisk, mkfs.fat, kexec, tar, systemd-boot, firmware and OVMF from nixpkgs. Each is replaced by a native plan in roadmap phase 5. |

`cargo make build` refuses a kernel whose bzImage release, module
directory and module vermagic disagree.
