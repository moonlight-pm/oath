# glibc and the Linux UAPI headers, built by pkg:gcc (R17). Headers in
# include/, libraries and the loader in lib/: slibdir and rtlddir are
# /lib, and there is no /lib64.
T=x86_64-pc-linux-gnu
tar -xf "$SRC/glibc-$VERSION.tar.xz"
tar -xf "$SRC/linux-7.3-rc1.tar.gz"
make -C linux-7.3-rc1 ARCH=x86_64 HOSTCC=cc INSTALL_HDR_PATH=/tmp/uapi headers_install
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
