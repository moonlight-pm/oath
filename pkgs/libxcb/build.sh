# libxcb and its extension libraries, shared, generated from xcb-proto.
tar -xf "$SRC/$SOURCES"
cd "libxcb-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --disable-devel-docs --without-doxygen
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR"/*.la
