# GMP, shared. For generic x86_64, not the build machine's CPU: the same
# plan gives the same product, and it runs on every Oath machine. GMP
# 6.3.0 predates C23, GCC 16's default: its configure tests need C17.
tar -xf "$SRC/$SOURCES"
cd "gmp-$VERSION"
CFLAGS="$CFLAGS -std=gnu17" ./configure --prefix="$LIVE" --build=x86_64-pc-linux-gnu \
	--host=x86_64-pc-linux-gnu --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
find /out/lib -name '*.la' -delete
