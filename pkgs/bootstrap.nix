{ pkgs ? import <nixpkgs> {} }:
# Host tools for pkgs/bootstrap.sh: the compiler and build tools that
# build stage 0 (R17). Build-time only; nothing from here ships in Oath.
pkgs.mkShell {
  packages = with pkgs; [
    gcc
    gnumake
    perl
    python3
    bison
    gawk
    m4
    xz
    bzip2
    gzip
    gnutar
    file
    which
  ];
  hardeningDisable = [ "all" ];
}
