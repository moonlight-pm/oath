# libexpat, shared, with headers and pkg-config; no xmlwf. patches/:
# upstream's fixes for 2.10.0, which no release has yet, in the order master
# took them: CVE-2026-115506 (amplification found late), CVE-2026-113351
# (XML_ParserReset accounting), CVE-2026-118370 (storeAtts overflow),
# CVE-2026-117610 (empty subparser parse).
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
