# readline 8.3 with its upstream patches, linked to libncursesw for its
# terminal calls.
tar -xf "$SRC/readline-$VERSION.tar.gz"
cd "readline-$VERSION"
for p in "$SRC"/readline83-*; do patch -p0 -i "$p"; done
./configure --prefix="$LIVE" --libdir="$LIVE/lib" --with-curses --disable-static --disable-install-examples
make -j"$JOBS" SHLIB_LIBS=-lncursesw
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
