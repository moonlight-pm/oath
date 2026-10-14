{ pkgs ? import <nixpkgs> {} }:
# Host tools for pkgs/bootstrap.sh: the compilers and build systems that
# build stage 0. Build-time only; nothing from here ships in Oath.
pkgs.mkShell {
  packages = with pkgs; [
    gcc
    cmake
    ninja
    python3
    gnumake
    perl
    xz
    bzip2
    gzip
    gnutar
    file
  ];
  hardeningDisable = [ "all" ];
}
