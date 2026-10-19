# libXcomposite, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "libXcomposite-$VERSION"
./configure --prefix="$LIVE" --disable-static --without-xmlto
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
