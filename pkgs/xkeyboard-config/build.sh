# Data only: share/xkeyboard-config-2, with share/X11/xkb linking to it.
# Its .pc goes to lib/pkgconfig, where the sandbox looks. No NLS. The
# sandbox has no /usr/bin/env: meson runs a script that is not
# executable through the perl on PATH.
tar -xf "$SRC/$SOURCES"
cd "xkeyboard-config-$VERSION"
chmod -x rules/xml2lst.pl
meson setup build --prefix="$LIVE" --buildtype=release -Dnls=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/share" /out/
mkdir -p /out/lib
mv /out/share/pkgconfig /out/lib/pkgconfig
rm -rf /out/share/man
