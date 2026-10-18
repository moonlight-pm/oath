# libxcvt, shared, and cvt.
tar -xf "$SRC/$SOURCES"
cd "libxcvt-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
