# libXxf86vm, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "libXxf86vm-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR"/*.la
