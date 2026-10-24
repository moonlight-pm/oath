# libdrm and the AMD drivers' libraries; libdrm_amdgpu reads
# share/libdrm/amdgpu.ids from this slot.
tar -xf "$SRC/$SOURCES"
cd "libdrm-$VERSION"
meson setup build --prefix="$LIVE" --libdir="$LIBDIR" --buildtype=release \
	-Damdgpu=enabled -Dradeon=enabled -Dintel=disabled -Dnouveau=disabled -Dvmwgfx=disabled \
	-Dfreedreno=disabled -Dvc4=disabled -Detnaviv=disabled -Domap=disabled -Dexynos=disabled \
	-Dtegra=disabled -Dcairo-tests=disabled -Dman-pages=disabled -Dvalgrind=disabled -Dtests=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" "/tmp/dest$LIVE/share" /out/
