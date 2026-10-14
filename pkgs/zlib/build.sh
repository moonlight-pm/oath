tar -xf "$SRC/$SOURCES"
cd zlib-$VERSION
./configure --prefix="$LIVE" --static
make -j"$JOBS" libz.a
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
