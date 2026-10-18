# MPFR, shared, against the GMP package.
tar -xf "$SRC/$SOURCES"
cd "mpfr-$VERSION"
./configure --prefix="$LIVE" --disable-static --with-gmp=/oath/store/pkg/gmp/live
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
find /out/lib -name '*.la' -delete
rm -rf /out/share
