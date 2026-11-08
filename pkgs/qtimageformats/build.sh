# Shared, with headers and CMake packages, into this pack, built against
# pkg:qtbase: the WebP plugin alone (plugins/imageformats/libqwebp.so); the
# others read formats nothing here needs, so they are not shipped.
tar -xf "$SRC/$SOURCES"
cd "qtimageformats-everywhere-src-$VERSION"
prefixes=$(printf %s "$PKG_CONFIG_PATH" | tr : '\n' | sed 's|/lib/pkgconfig$||' | paste -sd ';')
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_PREFIX_PATH="/oath/store/pkg/qtbase/live;$prefixes" \
	-DQT_BUILD_EXAMPLES=OFF -DQT_BUILD_TESTS=OFF \
	-DFEATURE_webp=ON -DFEATURE_system_webp=OFF -DFEATURE_tiff=OFF -DFEATURE_mng=OFF \
	-DFEATURE_jasper=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
[ -f /out/plugins/imageformats/libqwebp.so ]
find /out/plugins/imageformats -type f ! -name libqwebp.so -delete
find /out/share/cmake -name 'Qt6Q*Plugin*.cmake' ! -name 'Qt6QWebpPlugin*' -delete
[ ! -e /out/lib/cmake ]
