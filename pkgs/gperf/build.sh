# gperf.
tar -xf "$SRC/$SOURCES"
cd "gperf-$VERSION"
./configure --prefix="$LIVE"
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
