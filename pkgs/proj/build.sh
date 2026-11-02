# Shared, with headers, pkg-config and share/proj/proj.db; no programs or tests.
tar -xf "$SRC/$SOURCES"
cd "proj-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DCMAKE_PREFIX_PATH=/oath/store/pkg/sqlite/live \
	-DEXE_SQLITE3=/oath/store/pkg/sqlite/live/bin/sqlite3 -DENABLE_TIFF=OFF -DENABLE_CURL=OFF \
	-DBUILD_TESTING=OFF -DBUILD_APPS=OFF -DBUILD_SHARED_LIBS=ON
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf "/out/$LIBDIR/cmake" /out/share/man /out/share/doc
[ -s /out/share/proj/proj.db ]
