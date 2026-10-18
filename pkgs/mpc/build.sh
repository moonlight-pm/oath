# MPC, shared, against the GMP and MPFR packages.
tar -xf "$SRC/$SOURCES"
cd "mpc-$VERSION"
./configure --prefix="$LIVE" --disable-static --with-gmp=/oath/store/pkg/gmp/live \
	--with-mpfr=/oath/store/pkg/mpfr/live
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
find /out/lib -name '*.la' -delete
