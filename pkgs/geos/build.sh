# Shared, with headers, pkg-config and geos-config; no tests or geosop.
tar -xf "$SRC/$SOURCES"
cd "geos-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_TESTING=OFF -DBUILD_DOCUMENTATION=OFF -DBUILD_GEOSOP=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf "/out/$LIBDIR/cmake"
