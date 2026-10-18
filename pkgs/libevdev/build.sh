# libevdev, shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "libevdev-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dtests=disabled -Dtools=disabled -Ddocumentation=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
