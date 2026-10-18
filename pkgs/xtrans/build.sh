# Headers and C sources, with xtrans.pc in lib/pkgconfig, where the sandbox
# looks.
tar -xf "$SRC/$SOURCES"
cd "xtrans-$VERSION"
./configure --prefix="$LIVE" --disable-docs --without-xmlto --without-fop --without-xsltproc
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" /out/
mkdir -p /out/lib
cp -a "/tmp/dest$LIVE/share/pkgconfig" /out/lib/
