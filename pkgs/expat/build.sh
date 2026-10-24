# libexpat, shared, with headers and pkg-config; no xmlwf.
tar -xf "$SRC/$SOURCES"
cd "expat-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --without-xmlwf --without-examples \
	--without-tests --without-docbook
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -rf "/out/$LIBDIR/cmake" "/out/$LIBDIR"/*.la
