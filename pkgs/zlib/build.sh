# zlib, shared, with headers and pkg-config. patches/: upstream's fix for
# CVE-2026-85091 (develop df84af2, not in a release yet).
tar -xf "$SRC/$SOURCES"
cd "zlib-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR"
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
rm -f "/out/$LIBDIR/libz.a"
