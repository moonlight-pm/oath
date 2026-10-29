# libglvnd with EGL, GLX and GLES 2/3 (no GLES 1: Mesa has none). The
# EGL vendor directories are fixed here, as the Vulkan loader's are:
# /etc, then NVIDIA's slot, then Mesa's, so a machine with both tries
# NVIDIA's first and one without pkg:nvidia finds only Mesa's. Its tests,
# which always build, are left out: none runs here, and i386's would
# link against lib32/ libraries the linker does not search.
tar -xf "$SRC/$SOURCES"
cd "libglvnd-v$VERSION"
sed -i "/^subdir('tests')\$/d" meson.build
grep -qx "subdir('tests')" meson.build && exit 1
dirs=/etc/glvnd/egl_vendor.d:/oath/store/pkg/nvidia/live/share/glvnd/egl_vendor.d
dirs=$dirs:/oath/store/pkg/mesa/live/share/glvnd/egl_vendor.d
sed -i "s|'@0@/glvnd/egl_vendor.d:@1@/glvnd/egl_vendor.d'.format(|'$dirs'.format(|" src/EGL/meson.build
grep -qF "'$dirs'" src/EGL/meson.build
meson setup build --prefix="$LIVE" --libdir="$LIBDIR" --buildtype=release \
	-Dx11=enabled -Dglx=enabled -Degl=true -Dgles1=false -Dgles2=true -Dhgl=false -Dheaders=true
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
strings "/out/$LIBDIR/libEGL.so.1" | grep -qxF "$dirs"
