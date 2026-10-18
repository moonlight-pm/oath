# GCC (C, C++) and binutils, native, built by the previous pkg:gcc (R17).
# Every program it links names /lib/ld-linux-x86-64.so.2; it reads
# glibc's headers from /oath/store/pkg/glibc/live/include and finds
# glibc's libraries and startup files in /lib. It links the GMP, MPFR and
# MPC packages.
T=x86_64-pc-linux-gnu
for f in $SOURCES; do tar -xf "$SRC/$f"; done
g=gcc-$VERSION
sed -i 's|/lib64/ld-linux-x86-64.so.2|/lib/ld-linux-x86-64.so.2|' "$g/gcc/config/i386/linux64.h"
sed -i '/m64=/s/lib64/lib/' "$g/gcc/config/i386/t-linux64"
# The compiler builds on its own terms.
unset CPPFLAGS LDFLAGS
export CFLAGS=-O2 CXXFLAGS=-O2
mkdir b-binutils b-gcc
cd b-binutils
../binutils-*/configure --build=$T --host=$T --target=$T --prefix= --with-lib-path=/lib \
	--without-zstd --disable-nls --disable-werror --disable-gprofng --disable-gdb --disable-gdbserver \
	--disable-sim --disable-readline --disable-libdecnumber --enable-default-hash-style=gnu \
	--enable-plugins --enable-ld=default MAKEINFO=true
make -j"$JOBS" MAKEINFO=true
make install DESTDIR=/out MAKEINFO=true
cd ../b-gcc
"../$g/configure" --build=$T --host=$T --target=$T --prefix= \
	--disable-nls --disable-multilib --disable-bootstrap --enable-default-pie \
	--enable-default-ssp --enable-languages=c,c++ --disable-libsanitizer --disable-libssp \
	--disable-libvtv --disable-libitm --disable-libquadmath --without-isl --without-zstd \
	--with-native-system-header-dir=/oath/store/pkg/glibc/live/include \
	--with-gmp=/oath/store/pkg/gmp/live --with-mpfr=/oath/store/pkg/mpfr/live \
	--with-mpc=/oath/store/pkg/mpc/live \
	--enable-shared --enable-threads=posix --enable-__cxa_atexit --enable-lto \
	--with-gcc-major-version-only MAKEINFO=true
make -j"$JOBS" MAKEINFO=true
make install DESTDIR=/out MAKEINFO=true
cd /out
# Names build scripts expect, and binutils' plugin directory for LTO.
ln -sf gcc bin/cc
ln -sf g++ bin/c++
mkdir -p lib/bfd-plugins
ln -sf "../../libexec/gcc/$T/16/liblto_plugin.so" lib/bfd-plugins/liblto_plugin.so
# binutils' tooldir copies, its headers and the docs are not needed.
rm -rf "$T" share include/*.h
find lib -name '*.la' -delete
