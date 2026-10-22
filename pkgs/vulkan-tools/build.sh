# vulkaninfo, with X11 (xcb, Xlib) and Wayland surfaces.
tar -xf "$SRC/$SOURCES"
cd "Vulkan-Tools-$VERSION"
cmake -S . -B build -DCMAKE_INSTALL_PREFIX="$LIVE" -DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_PREFIX_PATH=/oath/store/pkg/vulkan-headers/live \
	-DBUILD_CUBE=OFF -DBUILD_ICD=OFF -DBUILD_TESTS=OFF -DBUILD_VULKANINFO=ON
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/bin" /out/
