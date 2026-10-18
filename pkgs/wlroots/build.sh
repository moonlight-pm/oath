# wlroots, shared, with Oath's patches: the DRM and libinput backends
# (session through libseat, devices through libudev-zero), the GLES2
# renderer, GBM buffers and Xwayland, whose path it compiles in.
tar -xf "$SRC/$SOURCES"
cd "wlroots-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release -Dwerror=false \
	-Dexamples=false -Dbackends=drm,libinput -Drenderers=gles2 -Dallocators=gbm \
	-Dsession=enabled -Dxwayland=enabled -Dxcb-errors=enabled -Dlibliftoff=disabled \
	-Dcolor-management=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
