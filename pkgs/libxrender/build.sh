# libXrender, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "libXrender-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR"/*.la
