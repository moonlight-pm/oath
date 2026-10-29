# libsbc, without its tools and tester.
tar -xf "$SRC/$SOURCES"
cd "sbc-$VERSION"
./configure --prefix="$LIVE" --disable-static --disable-tools --disable-tester
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
