# The Wayland libraries and wayland-scanner, without docs, tests or DTD
# validation (libxml2).
tar -xf "$SRC/$SOURCES"
cd "wayland-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Ddocumentation=false -Ddtd_validation=false -Dtests=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
