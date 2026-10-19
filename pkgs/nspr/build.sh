# NSPR, 64-bit, optimized, shared only.
tar -xf "$SRC/$SOURCES"
cd "nspr-$VERSION/nspr"
./configure --prefix="$LIVE" --with-mozilla --with-pthreads --enable-64bit --enable-optimize \
	--disable-debug
make
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.a
