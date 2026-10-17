# LZO, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "lzo-$VERSION"
./configure --prefix="$LIVE" --enable-shared --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
find /out/lib -name '*.la' -delete
