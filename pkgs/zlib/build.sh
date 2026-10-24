# zlib, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "zlib-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR"
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR/libz.a"
