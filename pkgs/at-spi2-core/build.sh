# at-spi2-core without X11, introspection, docs or systemd; the a11y bus
# is found over the session bus.
tar -xf "$SRC/$SOURCES"
cd "at-spi2-core-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dx11=disabled -Dintrospection=disabled -Ddocs=false -Duse_systemd=false \
	-Ddefault_bus=dbus-daemon -Ddbus_daemon=/bin/dbus-daemon -Dgtk2_atk_adaptor=false \
	-Ddbus_glib=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/lib/python* /out/share/locale
