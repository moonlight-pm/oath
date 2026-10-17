# GNU diffutils: diff, cmp, diff3, sdiff.
tar -xf "$SRC/$SOURCES"
cd "diffutils-$VERSION"
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
