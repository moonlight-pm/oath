# Mesa: libEGL, libGLESv2, libGL (GLX), libgbm and the radeonsi and
# softpipe drivers, on Wayland and X11. drirc's per-program workarounds
# are on (xmlconfig, expat).
tar -xf "$SRC/$SOURCES"
cd "mesa-$VERSION"
PYTHONPATH=
for m in mako markupsafe pyyaml packaging; do
	PYTHONPATH="$PYTHONPATH${PYTHONPATH:+:}/oath/store/pkg/python-$m/live/lib/python"
done
export PYTHONPATH
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nofallback \
	-Dplatforms=wayland,x11 -Dgallium-drivers=radeonsi,softpipe -Dvulkan-drivers= \
	-Dllvm=disabled -Damd-use-llvm=false -Ddraw-use-llvm=false \
	-Degl=enabled -Dgbm=enabled -Dgles2=enabled -Dgles1=disabled -Dopengl=true \
	-Dglx=dri -Dglvnd=disabled -Dxlib-lease=disabled \
	-Dgallium-va=disabled -Dgallium-rusticl=false -Dgallium-d3d10umd=false \
	-Dgallium-mediafoundation=disabled -Dgallium-d3d12-video=disabled -Dmicrosoft-clc=disabled \
	-Dvideo-codecs= -Dtools= -Dbuild-tests=false -Denable-glcpp-tests=false \
	-Dvalgrind=disabled -Dlibunwind=disabled -Dlmsensors=disabled -Dperfetto=false \
	-Dexpat=enabled -Dxmlconfig=enabled -Dzlib=enabled -Dzstd=enabled \
	-Ddisplay-info=disabled -Dspirv-tools=disabled -Dshader-cache=enabled -Dhtml-docs=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
