# GNU sed.
tar -xf "$SRC/$SOURCES"
cd "sed-$VERSION"
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
