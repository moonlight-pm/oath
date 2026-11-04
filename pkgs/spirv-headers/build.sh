# Data only: include/spirv and its CMake package.
tar -xf "$SRC/$SOURCES"
cd "SPIRV-Headers-vulkan-sdk-$VERSION"
cmake -S . -B build -DCMAKE_INSTALL_PREFIX="$LIVE" -DSPIRV_HEADERS_ENABLE_TESTS=OFF -DSPIRV_HEADERS_ENABLE_INSTALL=ON
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/include" /out/
mkdir -p /out/share
cp -a "/tmp/dest$LIVE/share/cmake" /out/share/
cp LICENSE /out/share/LICENSE-SPIRV-Headers
