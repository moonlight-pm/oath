# Shared, with headers, pkg-config, bin/cmark and its CMake package, which
# names that program; no tests. The package is in share/cmake: a pack's lib/
# is linked into /lib, which has one lib/cmake.
tar -xf "$SRC/$SOURCES"
cd "cmark-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man
mkdir -p /out/share
mv "/out/$LIBDIR/cmake" /out/share/cmake
[ -f /out/share/cmake/cmark/cmark-config.cmake ]
