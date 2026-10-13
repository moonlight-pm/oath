{ pkgs ? import <nixpkgs> {} }:
let
  muslCC = pkgs.pkgsStatic.stdenv.cc;
in
pkgs.mkShell {
  packages = [
    pkgs.qemu
    pkgs.btrfs-progs
    pkgs.pkgsStatic.busybox
    pkgs.pkgsStatic.btrfs-progs
    pkgs.cpio
    muslCC
  ];
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
    "${muslCC}/bin/${muslCC.targetPrefix}cc";
  OATH_BUSYBOX = "${pkgs.pkgsStatic.busybox}/bin/busybox";
  OATH_BTRFS = "${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs";
}
