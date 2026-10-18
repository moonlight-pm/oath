# libdisplay-info, shared, and di-edid-decode. The sandbox has no
# /usr/bin/env: a script that is not executable runs through meson's
# python instead.
tar -xf "$SRC/$SOURCES"
cd "libdisplay-info-$VERSION"
chmod -x tool/gen-search-table.py
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
