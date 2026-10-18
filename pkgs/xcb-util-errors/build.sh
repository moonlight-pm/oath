# libxcb-errors, shared, generated from xcb-proto's XML.
tar -xf "$SRC/$SOURCES"
cd "xcb-util-errors-$VERSION"
./configure --prefix="$LIVE" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
