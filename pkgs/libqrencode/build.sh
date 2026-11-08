# Shared, with its header and pkg-config; no qrencode program or tests. Its
# CMakeLists.txt (2020) asks for CMake 3.1's policies, which CMake 4 refuses.
tar -xf "$SRC/$SOURCES"
cd "libqrencode-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_SHARED_LIBS=YES -DWITH_TOOLS=NO -DWITH_TESTS=NO \
	-DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man
[ -f "/out/$LIBDIR/pkgconfig/libqrencode.pc" ]
