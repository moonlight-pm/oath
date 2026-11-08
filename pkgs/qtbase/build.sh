# Shared, with headers, CMake packages, mkspecs, its tools (bin/, libexec/:
# moc, rcc, uic) and plugins/ (platforms/libqwayland.so, tls/, imageformats/).
# Not relocatable: Qt finds its plugins and data at $LIVE, not beside wherever
# a link to its library is. Other Qt modules install into their own packs; a
# program names their plugins/ in QT_PLUGIN_PATH.
tar -xf "$SRC/$SOURCES"
cd "qtbase-everywhere-src-$VERSION"
# CA roots: pkg:certs' list first, then the files Qt reads under /etc.
certs=/oath/store/pkg/certs/live/ssl/cert.pem
f=src/plugins/tls/openssl/qtlsbackend_openssl.cpp
sed -i "s|^        QSet<QString> certFiles = {|&\n            QStringLiteral(\"$certs\"), // Oath's list (pkg:certs)|" "$f"
grep -q "$certs" "$f"
# CMake finds the library packs by prefix: each one on PKG_CONFIG_PATH.
prefixes=$(printf %s "$PKG_CONFIG_PATH" | tr : '\n' | sed 's|/lib/pkgconfig$||' | paste -sd ';')
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_PREFIX_PATH="$prefixes" \
	-DQT_BUILD_EXAMPLES=OFF -DQT_BUILD_TESTS=OFF -DQT_BUILD_BENCHMARKS=OFF -DFEATURE_relocatable=OFF \
	-DFEATURE_xcb=OFF -DFEATURE_xlib=OFF -DFEATURE_wayland=ON -DFEATURE_wayland_server=OFF \
	-DFEATURE_egl=ON -DINPUT_opengl=desktop -DFEATURE_vulkan=OFF \
	-DFEATURE_eglfs=OFF -DFEATURE_linuxfb=OFF -DFEATURE_vnc=OFF -DFEATURE_vkkhrdisplay=OFF \
	-DFEATURE_gbm=OFF -DFEATURE_kms=OFF -DFEATURE_libinput=OFF -DFEATURE_tslib=OFF \
	-DFEATURE_mtdev=OFF -DFEATURE_evdev=OFF -DFEATURE_libudev=OFF \
	-DFEATURE_dbus=OFF -DFEATURE_glib=OFF -DFEATURE_icu=OFF -DFEATURE_sql=OFF \
	-DFEATURE_printsupport=OFF -DFEATURE_cups=OFF -DFEATURE_journald=OFF \
	-DFEATURE_libproxy=OFF -DFEATURE_gssapi=OFF -DFEATURE_brotli=OFF \
	-DFEATURE_openssl_linked=ON -DFEATURE_fontconfig=ON -DFEATURE_zstd=ON \
	-DFEATURE_system_zlib=ON -DFEATURE_system_pcre2=OFF -DFEATURE_system_harfbuzz=ON \
	-DFEATURE_system_freetype=ON -DFEATURE_system_png=OFF -DFEATURE_system_jpeg=OFF
grep -q '^QT_FEATURE_wayland_client:INTERNAL=ON' build/CMakeCache.txt
grep -q '^QT_FEATURE_xcb:INTERNAL=OFF' build/CMakeCache.txt
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
[ -f /out/plugins/platforms/libqwayland.so ]
[ -f /out/plugins/tls/libqopensslbackend.so ]
"/out/bin/qtpaths" --query QT_INSTALL_PLUGINS | grep -qx "$LIVE/plugins"
