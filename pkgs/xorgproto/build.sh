# Headers, with their .pc files in lib/pkgconfig, where the sandbox looks.
tar -xf "$SRC/$SOURCES"
cd "xorgproto-$VERSION"
./configure --prefix="$LIVE" --disable-specs --without-xmlto --without-fop --without-xsltproc
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" /out/
mkdir -p /out/lib
cp -a "/tmp/dest$LIVE/share/pkgconfig" /out/lib/
