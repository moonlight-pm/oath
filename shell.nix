{ pkgs ? import <nixpkgs> {} }:
# Optional dev shell on the build host: QEMU and btrfs-progs for working
# with images by hand. Nothing here ships; `cargo make` finds its own
# host tools (image/host.nix) and builds everything that ships (pkgs/).
pkgs.mkShell {
  packages = [ pkgs.qemu pkgs.btrfs-progs ];
}
