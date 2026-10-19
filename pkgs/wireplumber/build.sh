# WirePlumber with its daemon, modules and wpctl; no introspection, docs,
# systemd or elogind. share/wireplumber/wireplumber.conf.d/50-oath.conf
# turns off what needs Bluetooth, logind or portals.
tar -xf "$SRC/wireplumber-$VERSION.tar.gz"
cd "wireplumber-$VERSION"
mkdir -p subprojects/packagecache
cp "$SRC/lua-5.5.0.tar.gz" "$SRC/lua_5.5.0-1_patch.zip" subprojects/packagecache/
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dsystem-lua=false -Dintrospection=disabled -Ddoc=disabled -Dsystemd=disabled \
	-Delogind=disabled -Dsystemd-user-service=false -Dsystemd-system-service=false \
	-Dtests=false -Ddbus-tests=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/lib/liblua* /out/include/lua* /out/lib/pkgconfig/lua* /out/share/locale /out/lib/cmake
mkdir -p /out/share/wireplumber/wireplumber.conf.d
cp "$FILES/50-oath.conf" /out/share/wireplumber/wireplumber.conf.d/
