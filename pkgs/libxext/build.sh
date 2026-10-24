# libXext, shared, with headers and pkg-config; no specs or man pages.
tar -xf "$SRC/$SOURCES"
cd "libXext-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --disable-specs --without-xmlto --without-fop \
	--without-xsltproc
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR"/*.la
