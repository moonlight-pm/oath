# Shared, with headers and CMake packages, into this pack (its plugins in
# plugins/), built against pkg:qtbase.
tar -xf "$SRC/$SOURCES"
cd "qtnetworkauth-everywhere-src-$VERSION"
prefixes=$(printf %s "$PKG_CONFIG_PATH" | tr : '\n' | sed 's|/lib/pkgconfig$||' | paste -sd ';')
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_PREFIX_PATH="/oath/store/pkg/qtbase/live;$prefixes" \
	-DQT_BUILD_EXAMPLES=OFF -DQT_BUILD_TESTS=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
[ ! -e /out/lib/cmake ]
