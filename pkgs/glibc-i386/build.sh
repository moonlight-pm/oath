# glibc for i386 (R17, R29). pkg:gcc compiles only x86_64 code with a
# libgcc to match, so this first builds a C compiler that also targets
# -m32 (static libgcc, no C library) and compiles glibc with it. slibdir
# is /lib32, so ${LIB} is lib32 and the loader searches /lib32; rtlddir is
# /lib, the loader's interoperable path that 32-bit programs name.
# patches/: pkgs/glibc's, the 2.44 branch.
T=x86_64-pc-linux-gnu
for f in $SOURCES; do tar -xf "$SRC/$f"; done
for p in "$FILES"/patches/*.patch; do
	patch -d "glibc-$VERSION" -p1 -s -i "$p"
done
make -C linux-7.3 ARCH=x86_64 HOSTCC=cc headers
find linux-7.3/usr/include -type f ! -name '*.h' -delete
mkdir -p /tmp/uapi
cp -r linux-7.3/usr/include /tmp/uapi/
rm -rf linux-7.3
unset CPPFLAGS LDFLAGS
export CFLAGS="-O2 -pipe" CXXFLAGS="-O2 -pipe"
# The compiler: C only, both ABIs, libgcc alone.
mkdir b-gcc
cd b-gcc
../gcc-16.2.0/configure --build=$T --host=$T --target=$T --prefix=/tmp/cc32 \
	--enable-multilib --with-multilib-list=m32,m64 --enable-languages=c \
	--with-newlib --without-headers --disable-shared --disable-threads \
	--disable-nls --disable-bootstrap --enable-default-pie --enable-default-ssp \
	--disable-libatomic --disable-libgomp --disable-libquadmath --disable-libssp \
	--disable-libvtv --disable-libstdcxx --disable-libsanitizer --disable-libitm \
	--without-isl --without-zstd --with-as=/bin/as --with-ld=/bin/ld \
	--with-gmp=/oath/store/pkg/gmp/live --with-mpfr=/oath/store/pkg/mpfr/live \
	--with-mpc=/oath/store/pkg/mpc/live --with-gcc-major-version-only MAKEINFO=true
make -j"$JOBS" all-gcc all-target-libgcc MAKEINFO=true
make install-gcc install-target-libgcc MAKEINFO=true
# Without shared libgcc the unwinder is in libgcc.a; glibc's static
# programs still ask for libgcc_eh.
for m in -m64 -m32; do
	d=$(/tmp/cc32/bin/gcc $m -print-libgcc-file-name)
	[ -e "${d%/*}/libgcc_eh.a" ] || ln -s libgcc.a "${d%/*}/libgcc_eh.a"
done
cd ..
rm -rf b-gcc gcc-16.2.0
# glibc, i386.
mkdir b-glibc
cd b-glibc
printf 'slibdir=/lib32\nrtlddir=/lib\ncomplocaledir=/lib/locale\n' >configparms
../glibc-$VERSION/configure --build=$T --host=i686-pc-linux-gnu --prefix= --libdir=/lib32 \
	--libexecdir=/lib32/libexec --with-headers=/tmp/uapi/include \
	--enable-kernel=5.10 --disable-werror --disable-nscd --disable-timezone-tools \
	--enable-stack-protector=strong --enable-bind-now \
	CC="/tmp/cc32/bin/gcc -m32" CXX=false AR=ar RANLIB=ranlib \
	libc_cv_slibdir=/lib32 libc_cv_rtlddir=/lib
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cd /tmp/dest
# It runs: the new loader starts the new getconf against the new libc.
./lib/ld-linux.so.2 --library-path lib32 bin/getconf GNU_LIBC_VERSION | grep -qx "glibc $VERSION"
mkdir -p /out/include/gnu
mv lib/ld-linux.so.2 lib32/
cp -a lib32 /out/
# The headers are pkg:glibc's: x86's are the same for both ABIs.
for h in stubs-32.h lib-names-32.h; do mv "include/gnu/$h" /out/include/gnu/; done
diff -r include /oath/store/pkg/glibc/live/include -x 'stubs-64.h' -x 'lib-names-64.h' \
	-x asm -x asm-generic -x linux -x drm -x mtd -x rdma -x scsi -x sound -x video -x xen \
	-x misc -x cxl -x fwctl -x regulator >/dev/null
# libc.so and friends name /lib32/… and /lib/ld-linux.so.2, as a sandbox
# shows them. The 64-bit tools, data and cache are pkg:glibc's.
rm -rf /out/lib32/libexec
find /out/lib32 -name '*.la' -delete
