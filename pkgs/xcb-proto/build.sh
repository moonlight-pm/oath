# share/xcb (the protocol XML) and the xcbgen module, which libxcb's build
# finds through xcb-proto.pc; the .pc goes to lib/pkgconfig, where the
# sandbox looks.
tar -xf "$SRC/$SOURCES"
cd "xcb-proto-$VERSION"
./configure --prefix="$LIVE"
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
mv /out/share/pkgconfig /out/lib/pkgconfig
