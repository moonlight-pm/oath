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
    pkgs.linuxPackages_6_12.kernel
    muslCC
  ];
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
    "${muslCC}/bin/${muslCC.targetPrefix}cc";
  OATH_KERNEL = "${pkgs.linuxPackages_6_12.kernel}/bzImage";
  OATH_MODULES = "${pkgs.linuxPackages_6_12.kernel.modules}/lib/modules";
  OATH_BUSYBOX = "${pkgs.pkgsStatic.busybox}/bin/busybox";
  OATH_BTRFS = "${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs";
}
