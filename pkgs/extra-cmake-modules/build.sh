# share/ECM: the modules and ECMConfig.cmake, without docs or tests.
tar -xf "$SRC/$SOURCES"
cd "extra-cmake-modules-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_INSTALL_PREFIX="$LIVE" -DBUILD_TESTING=OFF \
	-DBUILD_HTML_DOCS=OFF -DBUILD_MAN_DOCS=OFF -DBUILD_QTHELP_DOCS=OFF
cmake --build build
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
[ -f /out/share/ECM/cmake/ECMConfig.cmake ]
