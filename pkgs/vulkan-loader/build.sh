# libvulkan.so.1 with X11 (xcb, Xlib, Xrandr) and Wayland surfaces,
# which need only their headers. /etc stays the system directory, and
# Mesa's share/ and NVIDIA's (R34) replace /usr/local/share:/usr/share.
tar -xf "$SRC/$SOURCES"
cd "Vulkan-Loader-$VERSION"
cmake -S . -B build -DCMAKE_INSTALL_PREFIX="$LIVE" -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_PREFIX_PATH=/oath/store/pkg/vulkan-headers/live \
	-DCMAKE_INSTALL_SYSCONFDIR=/etc -DFALLBACK_CONFIG_DIRS=/etc/xdg \
	-DFALLBACK_DATA_DIRS=/oath/store/pkg/mesa/live/share:/oath/store/pkg/nvidia/live/share \
	-DBUILD_TESTS=OFF -DBUILD_WSI_XCB_SUPPORT=ON -DBUILD_WSI_XLIB_SUPPORT=ON \
	-DBUILD_WSI_XLIB_XRANDR_SUPPORT=ON -DBUILD_WSI_WAYLAND_SUPPORT=ON
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/lib" /out/
rm -rf /out/lib/cmake
strings /out/lib/libvulkan.so.1 | grep -qx /oath/store/pkg/mesa/live/share:/oath/store/pkg/nvidia/live/share
