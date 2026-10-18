# xkbcomp, reading xkeyboard-config's slot by default.
tar -xf "$SRC/$SOURCES"
cd "xkbcomp-$VERSION"
./configure --prefix="$LIVE" --with-xkb-config-root=/oath/store/pkg/xkeyboard-config/live/share/X11/xkb
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" /out/
