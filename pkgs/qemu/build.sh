# QEMU for the build host (R32): qemu-system-x86_64 with KVM and user
# networking (pkg:libslirp), and qemu-img. No display but none, no audio,
# no TLS, no Rust, no user-mode emulation. Configure runs meson from
# PATH, without a venv (patches/0001). The x86 firmware is the release's
# own: SeaBIOS and its VGA BIOSes, iPXE's ROMs, and edk2's OVMF
# (share/qemu/edk2-x86_64-code.fd, edk2-i386-vars.fd); the other
# architectures' firmware is dropped.
tar -xf "$SRC/$SOURCES"
cd "qemu-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
find . -name '*.py' -exec chmod -x {} +
./configure --prefix="$LIVE" --python=python3 --target-list=x86_64-softmmu \
	--disable-download --disable-docs --disable-werror --disable-rust \
	--enable-kvm --enable-slirp --enable-tools --disable-guest-agent --disable-user \
	--disable-gtk --disable-sdl --disable-vnc --disable-opengl --disable-spice \
	--disable-curses --disable-gnutls --disable-nettle --disable-gcrypt \
	--disable-libudev --disable-xkbcommon --disable-fdt --disable-capstone \
	--disable-pa --disable-pipewire --disable-alsa --disable-oss --disable-jack \
	--disable-sndio --disable-libusb --disable-usb-redir --disable-virglrenderer
make -j"$JOBS"
make install DESTDIR=/tmp/dest
d=/tmp/dest$LIVE/share/qemu
rm -rf "$d"/dtb "$d"/ast27x0_bootrom.bin "$d"/openbios-* "$d"/QEMU,*.bin "$d"/qemu-nsis.bmp \
	"$d"/s390-ccw.img "$d"/slof.bin "$d"/skiboot.lid "$d"/pnv-pnor.bin "$d"/palcode-clipper \
	"$d"/u-boot* "$d"/qemu_vga.ndrv "$d"/hppa-firmware* "$d"/opensbi-* "$d"/npcm*_bootrom.bin \
	"$d"/vof*.bin "$d"/edk2-aarch64-* "$d"/edk2-arm-* "$d"/edk2-riscv-* "$d"/edk2-loongarch64-*
rm -rf "/tmp/dest$LIVE/share/doc" "/tmp/dest$LIVE/share/man" "/tmp/dest$LIVE/share/icons" \
	"/tmp/dest$LIVE/share/applications" "/tmp/dest$LIVE/include"
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/share" /out/
[ ! -d "/tmp/dest$LIVE/libexec" ] || cp -a "/tmp/dest$LIVE/libexec" /out/
