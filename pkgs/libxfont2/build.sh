# libXfont2, shared: the built-in fonts and the PCF and BDF readers, no
# FreeType.
tar -xf "$SRC/$SOURCES"
cd "libXfont2-$VERSION"
./configure --prefix="$LIVE" --disable-static --disable-freetype --disable-devel-docs \
	--without-xmlto --without-fop
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
