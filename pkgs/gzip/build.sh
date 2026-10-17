# GNU gzip.
tar -xf "$SRC/$SOURCES"
cd "gzip-$VERSION"
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
