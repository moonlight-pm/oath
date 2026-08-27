{ pkgs ? import <nixpkgs> {} }:
let
  k = pkgs.linuxPackages_6_12.kernel;
  muslCC = pkgs.pkgsStatic.stdenv.cc;
in
pkgs.runCommand "oath-build-tools" { } ''
  mkdir -p $out/bin
  ln -s ${k}/bzImage $out/bzImage
  ln -s ${k.modules}/lib/modules $out/modules
  ln -s ${pkgs.pkgsStatic.busybox}/bin/busybox $out/busybox
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs $out/btrfs
  ln -s ${muslCC}/bin/${muslCC.targetPrefix}cc $out/musl-cc
  for p in ${pkgs.qemu} ${pkgs.btrfs-progs} ${pkgs.cpio} ${pkgs.xz} ${pkgs.gzip}; do
    if [ -d $p/bin ]; then
      for b in $p/bin/*; do
        ln -s $b $out/bin/$(basename $b) || true
      done
    fi
  done
''
