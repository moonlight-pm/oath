# Data only. Its .pc goes to lib/pkgconfig, where the sandbox looks.
tar -xf "$SRC/$SOURCES"
cd "wayland-protocols-$VERSION"
meson setup build --prefix="$LIVE" --buildtype=release -Dtests=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
mkdir -p /out/lib
mv /out/share/pkgconfig /out/lib/pkgconfig
