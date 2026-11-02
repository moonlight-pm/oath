# Shared, with headers and pkg-config.
tar -xf "$SRC/$SOURCES"
cd "json-c-json-c-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_STATIC_LIBS=OFF -DBUILD_TESTING=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf "/out/$LIBDIR/cmake"
