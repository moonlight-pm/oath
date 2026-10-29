# libepoxy, shared, for EGL. It opens libEGL, and libGL for a desktop GL
# context, at run time, so it runs with libglvnd (R34). The sandbox has no
# /usr/bin/env: the generator, not executable, runs through meson's python.
tar -xf "$SRC/$SOURCES"
cd "libepoxy-$VERSION"
chmod -x src/gen_dispatch.py
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Degl=yes -Dglx=no -Dx11=false -Dtests=false -Ddocs=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
