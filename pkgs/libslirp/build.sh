# libslirp, shared, with headers and pkg-config; no fuzzers or tests.
tar -xf "$SRC/$SOURCES"
cd "libslirp-v$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
