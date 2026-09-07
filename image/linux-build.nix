{ pkgs ? import <nixpkgs> {} }:
# Host deps to compile vanilla Linux (image/build-linux.sh). Not the product kernel.
pkgs.mkShell {
  packages = with pkgs; [
    gcc
    gnumake
    flex
    bison
    bc
    python3
    perl
    gawk
    openssl
    ncurses
    elfutils
    zlib
    pkg-config
    rsync
    pahole
    binutils
    curl
    gnutar
    gzip
    patch
  ];
}
