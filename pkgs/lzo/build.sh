tar -xf "$SRC/$SOURCES"
cd lzo-$VERSION
./configure --prefix="$LIVE" --disable-shared --enable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
