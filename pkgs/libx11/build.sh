# libX11 and libX11-xcb, shared, and share/X11 (compose and locale data,
# XErrorDB). No specs or man pages.
tar -xf "$SRC/$SOURCES"
cd "libX11-$VERSION"
./configure --prefix="$LIVE" --disable-static --disable-specs --without-xmlto --without-fop \
	--without-xsltproc --without-perl
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
rm -f /out/lib/*.la
rm -rf /out/share/man /out/share/doc
