# Shared, with headers, pkg-config, its CMake package and bin/cmark, which
# that package names; no tests.
tar -xf "$SRC/$SOURCES"
cd "cmark-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man
[ -f "/out/$LIBDIR/cmake/cmark/cmark-config.cmake" ]
