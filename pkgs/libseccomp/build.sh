# Shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "libseccomp-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf "/out/$LIBDIR"/*.la /out/share/man
