{ pkgs ? import <nixpkgs> {} }:
let
  muslCC = pkgs.pkgsStatic.stdenv.cc;
  k =
    if pkgs ? linuxPackages_testing then pkgs.linuxPackages_testing.kernel
    else if pkgs ? linuxPackages_7_3 then pkgs.linuxPackages_7_3.kernel
    else if pkgs ? linuxPackages_7_2 then pkgs.linuxPackages_7_2.kernel
    else pkgs.linuxPackages_latest.kernel;
in
pkgs.mkShell {
  packages = [
    pkgs.qemu
    pkgs.btrfs-progs
    pkgs.pkgsStatic.busybox
    pkgs.pkgsStatic.btrfs-progs
    pkgs.cpio
    k
    muslCC
  ];
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
    "${muslCC}/bin/${muslCC.targetPrefix}cc";
  OATH_KERNEL = "${k}/bzImage";
  OATH_MODULES = "${k.modules}/lib/modules";
  OATH_BUSYBOX = "${pkgs.pkgsStatic.busybox}/bin/busybox";
  OATH_BTRFS = "${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs";
}
