# XZ Utils: xz and liblzma (shared, with headers and pkg-config).
tar -xf "$SRC/$SOURCES"
cd "xz-$VERSION"
./configure --prefix="$LIVE" --disable-nls --disable-static --disable-doc
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
find /out/lib -name '*.la' -delete
