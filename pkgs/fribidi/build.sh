# libfribidi; no docs (c2man), tool or tests.
tar -xf "$SRC/$SOURCES"
cd "fribidi-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload -Ddocs=false -Dbin=false -Dtests=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
