{ pkgs ? import <nixpkgs> {} }:
let
  k = pkgs.linuxPackages_6_12.kernel;
  muslCC = pkgs.pkgsStatic.stdenv.cc;
  # Same nixpkgs pin Sola uses so River 0.4.5 + wlroots 0.20 match the forks.
  pinned = import (builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/d233902339c02a9c334e7e593de68855ad26c4cb.tar.gz";
    sha256 = "sha256-30sZNZoA1cqF5JNO9fVX+wgiQYjB7HJqqJ4ztCDeBZE=";
  }) {};
  riverPack = pinned.callPackage ./river-pack.nix {
    riverSrc = ../forks/river;
    wlrootsSrc = ../forks/wlroots;
  };
  solaRt = pkgs.callPackage ./sola-rt.nix { };
in
pkgs.runCommand "oath-build-tools" { } ''
  mkdir -p $out/bin
  ln -s ${k}/bzImage $out/bzImage
  ln -s ${k.modules}/lib/modules $out/modules
  ln -s ${pkgs.pkgsStatic.busybox}/bin/busybox $out/busybox
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs $out/btrfs
  ln -s ${pkgs.pkgsStatic.dropbear}/bin/dropbear $out/dropbear
  ln -s ${pkgs.pkgsStatic.dropbear}/bin/dropbearkey $out/dropbearkey
  ln -s ${muslCC}/bin/${muslCC.targetPrefix}cc $out/musl-cc
  ln -s ${riverPack}/glibc $out/glibc
  ln -s ${riverPack}/river $out/river
  ln -s ${solaRt} $out/sola-rt
  for p in ${pkgs.qemu} ${pkgs.btrfs-progs} ${pkgs.cpio} ${pkgs.xz} ${pkgs.gzip} ${pkgs.patchelf} ${pkgs.file}; do
    if [ -d $p/bin ]; then
      for b in $p/bin/*; do
        ln -s $b $out/bin/$(basename $b) || true
      done
    fi
  done
''
