# Shared, with headers and pkg-config; no samples, tests or tools' man pages.
tar -xf "$SRC/$SOURCES"
cd icu/source
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --disable-samples \
	--disable-tests --disable-extras --disable-layoutex
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man "/out/$LIBDIR/icu"
