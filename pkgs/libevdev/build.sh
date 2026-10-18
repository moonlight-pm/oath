# libevdev, shared, with headers and pkg-config. The sandbox has no
# /usr/bin/env: a script that is not executable runs through the python
# on PATH instead.
tar -xf "$SRC/$SOURCES"
cd "libevdev-$VERSION"
chmod -x libevdev/make-event-names.py
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dtests=disabled -Dtools=disabled -Ddocumentation=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
