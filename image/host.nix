{ pkgs ? import <nixpkgs> {} }:
# Build-host tools that never ship in Oath: QEMU and its UEFI firmware, for
# `cargo make run | up | start | check | install --qemu`. Everything that
# ships is a native pack (pkgs/).
pkgs.runCommand "oath-host-tools" { } ''
  mkdir -p $out/bin
  ln -s ${pkgs.OVMF.firmware} $out/OVMF_CODE.fd
  ln -s ${pkgs.OVMF.variables} $out/OVMF_VARS.fd
  for b in ${pkgs.qemu}/bin/*; do
    ln -s $b $out/bin/$(basename $b)
  done
''
