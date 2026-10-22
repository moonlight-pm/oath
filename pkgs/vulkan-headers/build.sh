# Data only: include/ and share/cmake/VulkanHeaders. Not the registry
# XML (share/vulkan/registry), which only code generators read.
tar -xf "$SRC/$SOURCES"
cd "Vulkan-Headers-$VERSION"
cmake -S . -B build -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DVULKAN_HEADERS_ENABLE_MODULE=OFF -DVULKAN_HEADERS_ENABLE_TESTS=OFF
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/share" /out/
rm -rf /out/share/vulkan
