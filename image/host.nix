{ pkgs ? import <nixpkgs> {} }:
let
  muslCC = pkgs.pkgsStatic.stdenv.cc;
  # Dropbear looks up sftp-server at compile time. Nixpkgs defaults that
  # to a NixOS path; point it at the Oath /bin farm. enableSCP builds the
  # legacy scp helper so `scp -O` works too (modern scp uses SFTP).
  dropbearOath = pkgs.pkgsStatic.dropbear.override {
    enableSCP = true;
    sftpPath = "/bin/sftp-server";
  };
  sbEfi = pkgs.runCommand "systemd-bootx64.efi" { } ''
    cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootx64.efi $out
  '';
in
pkgs.runCommand "oath-build-tools" { } ''
  mkdir -p $out/bin $out/firmware
  ln -s ${pkgs.pkgsStatic.busybox}/bin/busybox $out/busybox
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/btrfs $out/btrfs
  ln -s ${pkgs.pkgsStatic.btrfs-progs}/bin/mkfs.btrfs $out/mkfs.btrfs
  ln -s ${dropbearOath}/bin/dropbear $out/dropbear
  ln -s ${dropbearOath}/bin/dropbearkey $out/dropbearkey
  ln -s ${dropbearOath}/bin/dbclient $out/dropbear-dbclient
  ln -s ${dropbearOath}/bin/scp $out/dropbear-scp
  ln -s ${pkgs.pkgsStatic.openssh}/bin/ssh $out/openssh-ssh
  ln -s ${pkgs.pkgsStatic.openssh}/bin/ssh-keygen $out/openssh-ssh-keygen
  ln -s ${pkgs.pkgsStatic.openssh}/libexec/sftp-server $out/sftp-server
  ln -s ${pkgs.pkgsStatic.gptfdisk}/bin/sgdisk $out/sgdisk
  ln -s ${pkgs.pkgsStatic.dosfstools}/bin/mkfs.fat $out/mkfs.fat
  ln -s ${pkgs.pkgsStatic.kexec-tools}/bin/kexec $out/kexec
  ln -s ${pkgs.pkgsStatic.gnutar}/bin/tar $out/gnutar
  ln -s ${sbEfi} $out/systemd-bootx64.efi
  ln -s ${pkgs.OVMF.firmware} $out/OVMF_CODE.fd
  ln -s ${pkgs.OVMF.variables} $out/OVMF_VARS.fd
  ln -s ${muslCC}/bin/${muslCC.targetPrefix}cc $out/musl-cc
  ln -s ${pkgs.pkgsStatic.curl.bin}/bin/curl $out/curl
  ln -s ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt $out/ca-bundle.crt
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
