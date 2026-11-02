# Shared, with headers, pkg-config and xml2-config; no Python module.
tar -xf "$SRC/$SOURCES"
cd "libxml2-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --without-python
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man /out/share/doc /out/share/gtk-doc "/out/$LIBDIR"/*.la "/out/$LIBDIR/cmake"
