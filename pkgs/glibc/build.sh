# glibc and the Linux UAPI headers, built by pkg:gcc (R17). Headers in
# include/, libraries and the loader in lib/: slibdir and rtlddir are
# /lib, and there is no /lib64.
T=x86_64-pc-linux-gnu
tar -xf "$SRC/glibc-$VERSION.tar.xz"
tar -xf "$SRC/linux-7.3-rc1.tar.gz"
# The UAPI headers, as headers_install makes them but without rsync.
make -C linux-7.3-rc1 ARCH=x86_64 HOSTCC=cc headers
find linux-7.3-rc1/usr/include -type f ! -name '*.h' -delete
mkdir -p /tmp/uapi
cp -r linux-7.3-rc1/usr/include /tmp/uapi/
# glibc builds on its own terms: no other pack's headers or libraries,
# and it requires optimization.
unset CPPFLAGS LDFLAGS
export CFLAGS="-O2 -pipe" CXXFLAGS="-O2 -pipe"
mkdir build
cd build
printf 'slibdir=/lib\nrtlddir=/lib\n' >configparms
../glibc-$VERSION/configure --build=$T --host=$T --prefix= --libdir=/lib \
	--libexecdir=/lib/libexec --with-headers=/tmp/uapi/include \
	--enable-kernel=5.10 --disable-werror --disable-nscd --disable-timezone-tools \
	--enable-stack-protector=strong --enable-bind-now CXX=false \
	libc_cv_slibdir=/lib libc_cv_rtlddir=/lib
make -j"$JOBS"
make install DESTDIR=/out
cp -a /tmp/uapi/include/. /out/include/
# A loader cache of the build's own tree is no use on a machine.
rm -rf /out/etc/ld.so.cache /out/var
# ldd lists the loaders of the usual layout (/lib64/…); Oath's is /lib's.
sed -i 's|^RTLDLIST=.*|RTLDLIST="/lib/ld-linux-x86-64.so.2"|' /out/bin/ldd
grep -qx 'RTLDLIST="/lib/ld-linux-x86-64.so.2"' /out/bin/ldd
