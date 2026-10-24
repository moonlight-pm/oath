# libcrypt.so.1 with glibc's symbol versions and every hash glibc's had,
# shared, with crypt.h; not libcrypt.so.2 or the static library.
tar -xf "$SRC/$SOURCES"
cd "libxcrypt-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --disable-werror \
	--enable-hashes=strong,glibc --enable-obsolete-api=glibc --disable-failure-tokens
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR"/*.la
test -e "/out/$LIBDIR/libcrypt.so.1"
