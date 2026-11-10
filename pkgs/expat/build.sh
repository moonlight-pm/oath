# libexpat, shared, with headers and pkg-config; no xmlwf. patches/:
# upstream's fix for 2.11.0, which no release has yet: CVE-2026-124317 (an
# unknown encoding's converter read past the input).
tar -xf "$SRC/$SOURCES"
cd "expat-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --without-xmlwf --without-examples \
	--without-tests --without-docbook
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -rf "/out/$LIBDIR/cmake" "/out/$LIBDIR"/*.la
