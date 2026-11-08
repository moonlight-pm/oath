# include/toml++ and its CMake package.
tar -xf "$SRC/$SOURCES"
cd "tomlplusplus-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_INSTALL_PREFIX="$LIVE" -DCMAKE_INSTALL_LIBDIR="$LIBDIR"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
[ -f /out/include/toml++/toml.hpp ]
