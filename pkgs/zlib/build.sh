# zlib, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "zlib-$VERSION"
./configure --prefix="$LIVE"
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/libz.a
