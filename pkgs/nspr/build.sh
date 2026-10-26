# NSPR, 64-bit, optimized, shared only. Its libraries record a build
# time (_BUILD_STRING, _BUILD_TIME); that is the source's configure date,
# not the clock, so every build is the same.
tar -xf "$SRC/$SOURCES"
cd "nspr-$VERSION/nspr"
./configure --prefix="$LIVE" --with-mozilla --with-pthreads --enable-64bit --enable-optimize \
	--disable-debug
t=$(stat -c %Y configure)
make SH_DATE="$(date -u -d "@$t" '+%Y-%m-%d %T')" SH_NOW="${t}000000"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.a
