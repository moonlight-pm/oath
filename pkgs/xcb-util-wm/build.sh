# libxcb-icccm and libxcb-ewmh, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "xcb-util-wm-$VERSION"
./configure --prefix="$LIVE" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
