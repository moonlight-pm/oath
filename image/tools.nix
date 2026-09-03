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
  sbEfi = pkgs.runCommand "systemd-bootx64.efi" { } ''
    cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootx64.efi $out
  '';
in
pkgs.runCommand "oath-build-tools" { } ''
  mkdir -p $out/bin $out/firmware
  ln -s ${k}/bzImage $out/bzImage
  ln -s ${k.modules}/lib/modules $out/modules
  ln -s ${pkgs.pkgsStatic.busybox}/bin/busybox $out/busybox
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs $out/btrfs
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/mkfs.btrfs $out/mkfs.btrfs
  ln -s ${pkgs.pkgsStatic.dropbear}/bin/dropbear $out/dropbear
  ln -s ${pkgs.pkgsStatic.dropbear}/bin/dropbearkey $out/dropbearkey
  ln -s ${pkgs.pkgsStatic.gptfdisk}/bin/sgdisk $out/sgdisk
  ln -s ${pkgs.pkgsStatic.dosfstools}/bin/mkfs.fat $out/mkfs.fat
  ln -s ${pkgs.pkgsStatic.kexec-tools}/bin/kexec $out/kexec
  ln -s ${pkgs.pkgsStatic.gnutar}/bin/tar $out/gnutar
  ln -s ${sbEfi} $out/systemd-bootx64.efi
  ln -s ${pkgs.OVMF.firmware} $out/OVMF_CODE.fd
  ln -s ${pkgs.OVMF.variables} $out/OVMF_VARS.fd
  ln -s ${muslCC}/bin/${muslCC.targetPrefix}cc $out/musl-cc
  ln -s ${riverPack}/glibc $out/glibc
  ln -s ${riverPack}/river $out/river
  ln -s ${solaRt} $out/sola-rt
  ln -s ${pkgs.git} $out/git
  ln -s ${pkgs.pkgsStatic.curl.bin}/bin/curl $out/curl
  ln -s ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt $out/ca-bundle.crt
  ln -s ${pkgs.pipewire} $out/pipewire
  ln -s ${pkgs.wireplumber} $out/wireplumber
  ln -s ${pkgs.alsa-lib} $out/alsa-lib
  ln -s ${pkgs.libpulseaudio} $out/libpulseaudio
  if [ -d ${pkgs.linux-firmware}/lib/firmware/tigon ]; then
    cp -a ${pkgs.linux-firmware}/lib/firmware/tigon $out/firmware/tigon
  fi
  # Mac Pro 2013 (canto): dual Pitcairn (FirePro / HD 7800). SI needs these
  # blobs plus amdgpu.si_support=1.
  mkdir -p $out/firmware/radeon $out/firmware/amdgpu
  # linux-firmware uses relative symlinks (pitcairn_me.bin -> hainan_me.bin).
  # Copy the blob, not the link, so the initrd can request_firmware.
  find ${pkgs.linux-firmware}/lib/firmware/radeon -iname '*pitcairn*' -exec cp -L {} $out/firmware/radeon/ \;
  find ${pkgs.linux-firmware}/lib/firmware/amdgpu -iname '*pitcairn*' -exec cp -L {} $out/firmware/amdgpu/ \;
  for p in ${pkgs.qemu} ${pkgs.btrfs-progs} ${pkgs.cpio} ${pkgs.xz} ${pkgs.gzip} ${pkgs.patchelf} ${pkgs.file}; do
    if [ -d $p/bin ]; then
      for b in $p/bin/*; do
        ln -s $b $out/bin/$(basename $b) || true
      done
    fi
  done
''
