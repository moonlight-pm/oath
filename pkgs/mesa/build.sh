# Mesa as libglvnd's vendor (R34): libEGL_mesa, libGLX_mesa and libgbm,
# with the radeonsi and softpipe drivers, on Wayland and X11; libEGL,
# libGL and libGLESv2 are pkg:libglvnd's, which finds Mesa by
# share/glvnd/egl_vendor.d/50_mesa.json. drirc's per-program workarounds
# are on (xmlconfig, expat). RADV, the Vulkan driver for amdgpu, and the
# device-select layer, which puts the compositor's GPU first: their
# manifests (share/vulkan) name each library by its name, found in /lib.
# libgbm loads a GPU's own backend (<driver>_gbm.so) from its lib/gbm
# or NVIDIA's slot ($LIBDIR/nvidia/gbm), and else its DRI one.
tar -xf "$SRC/$SOURCES"
cd "mesa-$VERSION"
PYTHONPATH=
for m in mako markupsafe pyyaml packaging; do
	PYTHONPATH="$PYTHONPATH${PYTHONPATH:+:}/oath/store/pkg/python-$m/live/lib/python"
done
export PYTHONPATH
gbm=$LIVE/$LIBDIR/gbm:/oath/store/pkg/nvidia/live/$LIBDIR/nvidia/gbm
meson setup build --prefix="$LIVE" --libdir="$LIBDIR" --buildtype=release --wrap-mode=nofallback \
	-Dplatforms=wayland,x11 -Dgallium-drivers=radeonsi,softpipe \
	-Dvulkan-drivers=amd -Dvulkan-layers=device-select -Dvulkan-manifest-per-architecture=false \
	-Dllvm=disabled -Damd-use-llvm=false -Ddraw-use-llvm=false \
	-Degl=enabled -Dgbm=enabled -Dgles2=enabled -Dgles1=disabled -Dopengl=true \
	-Dglx=dri -Dglvnd=enabled -Dgbm-backends-path="$gbm" -Dxlib-lease=disabled \
	-Dgallium-va=disabled -Dgallium-rusticl=false -Dgallium-d3d10umd=false \
	-Dgallium-mediafoundation=disabled -Dgallium-d3d12-video=disabled -Dmicrosoft-clc=disabled \
	-Dvideo-codecs= -Dtools= -Dbuild-tests=false -Denable-glcpp-tests=false \
	-Dvalgrind=disabled -Dlibunwind=disabled -Dlmsensors=disabled -Dperfetto=false \
	-Dexpat=enabled -Dxmlconfig=enabled -Dzlib=enabled -Dzstd=enabled \
	-Ddisplay-info=disabled -Dspirv-tools=disabled -Dshader-cache=enabled -Dhtml-docs=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" "/tmp/dest$LIVE/share" /out/
grep -q '"library_path": "libvulkan_radeon.so"' /out/share/vulkan/icd.d/radeon_icd.json
grep -q '"library_path" : "libEGL_mesa.so.0"' /out/share/glvnd/egl_vendor.d/50_mesa.json
strings "/out/$LIBDIR/libgbm.so.1" | grep -qxF "$gbm"
