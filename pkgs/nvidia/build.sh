# NVIDIA's driver from its .run, without its installer: the payload is a
# zstd tar after the stub's `skip` lines.
# - The closed kernel modules (nvidia, nvidia-modeset, nvidia-drm,
#   nvidia-uvm), built by pkg:gcc against pkg:linux-build, in
#   lib/modules/<release>/ with Oath's index (modules.dep names each
#   module's needs, pkg:linux's among them; PID 1 merges it).
# - Its libraries in lib/, linked by soname. libglvnd's own (libGL,
#   libEGL, libGLX, libOpenGL, libGLdispatch, libGLES*) come from
#   pkg:libglvnd instead, and the X server's, GTK's, OpenCL's loader and
#   confidential computing's are left out.
# - Its 32-bit libraries in lib32/, likewise (R29: Steam's sandbox puts
#   every pack's lib32/ at /lib32; pkg:libglvnd-i386 dispatches to them).
# - The manifests in share/: Vulkan's ICD and layers, glvnd's EGL vendor.
# - NVIDIA's own lookups under lib/nvidia/ (linked as /lib/nvidia): EGL's
#   external platforms (Wayland, GBM, X), whose second directory
#   libEGL_nvidia names as /usr/share/egl/egl_external_platform.d and is
#   rewritten in place to /lib/nvidia's, and GBM's backend
#   (nvidia-drm_gbm.so), which libgbm finds there. Mesa has lib/gbm.
#   The 32-bit backend is lib32/nvidia/gbm, where pkg:mesa-i386 looks.
# - DLSS for Proton: NVIDIA's NGX DLLs in lib/nvidia/wine/, beside
#   libGLX_nvidia, where NVIDIA's installer puts them and Proton looks.
# - bin/nvidia-smi, its loader name rewritten to /lib's (R17).
# - GSP firmware in lib/firmware/nvidia/<version>/.
run=$SRC/NVIDIA-Linux-x86_64-$VERSION.run
skip=$(sed -n '1,20s/^skip=//p' "$run")
mkdir -p /tmp/nv
tail -n +"$skip" "$run" | zstd -dc | tar -x --no-same-owner -C /tmp/nv
cd /tmp/nv
[ -f kernel/nvidia/nv-kernel.o_binary ]

K=/oath/store/pkg/linux-build/live/build
rel=$(cat "$K/kernelrelease")
make -C kernel -j"$JOBS" SYSSRC="$K" SYSOUT="$K" modules
m=/out/lib/modules/$rel
mkdir -p "$m"
for k in nvidia nvidia-modeset nvidia-drm nvidia-uvm; do
	strip --strip-debug -o "$m/$k.ko" "kernel/$k.ko"
done
cd "$m"
: >modules.dep
: >modules.alias
: >modules.softdep
for p in nvidia nvidia-modeset nvidia-drm nvidia-uvm; do
	objcopy -O binary --only-section=.modinfo "$p.ko" /tmp/modinfo
	tr '\0' '\n' </tmp/modinfo >/tmp/modinfo.txt
	grep -qx "vermagic=$rel .*" /tmp/modinfo.txt
	n=$(echo "$p" | tr - _)
	needs=$(sed -n 's/^depends=//p' /tmp/modinfo.txt | tr , ' ')
	echo "$p.ko:$(for d in $needs; do printf ' %s.ko' "$d"; done)" >>modules.dep
	sed -n "s/^alias=\(.*\)/alias \1 $n/p" /tmp/modinfo.txt >>modules.alias
	sed -n "s/^softdep=\(.*\)/softdep $n \1/p" /tmp/modinfo.txt >>modules.softdep
done
# Only nvidia has device aliases; the rest load after it, as NVIDIA's
# installer has its modprobe configuration do.
echo "softdep nvidia post: nvidia_modeset nvidia_uvm nvidia_drm" >>modules.softdep
cd /tmp/nv

mkdir -p /out/lib/nvidia/gbm /out/lib/nvidia/egl_external_platform.d /out/bin
for f in lib*.so.*; do
	case $f in
	libGL.so.* | libEGL.so.* | libGLX.so.* | libOpenGL.so.* | libGLdispatch.so.* | \
		libGLESv1_CM.so.* | libGLESv2.so.* | libOpenCL.so.* | libglxserver_nvidia.so.* | \
		libnvidia-gtk2.so.* | libnvidia-gtk3.so.* | libnvidia-pkcs11*.so.* | \
		libcudadebugger.so.*) continue ;;
	esac
	cp "$f" /out/lib/
	so=$(readelf -d "$f" | sed -n 's/.*(SONAME).*\[\(.*\)\]/\1/p')
	[ -z "$so" ] || [ "$so" = "$f" ] || ln -sf "$f" "/out/lib/$so"
done
ln -s libcuda.so.1 /out/lib/libcuda.so
ln -s ../../libnvidia-allocator.so.1 /out/lib/nvidia/gbm/nvidia-drm_gbm.so
cp [0-9][0-9]_nvidia_*.json /out/lib/nvidia/egl_external_platform.d/
mkdir -p /out/lib/nvidia/wine
cp nvngx.dll _nvngx.dll nvngx_dlssg.dll /out/lib/nvidia/wine/
mkdir -p /out/lib32
for f in 32/lib*.so.*; do
	b=${f#32/}
	case $b in
	libGL.so.* | libEGL.so.* | libGLX.so.* | libOpenGL.so.* | libGLdispatch.so.* | \
		libGLESv1_CM.so.* | libGLESv2.so.* | libOpenCL.so.*) continue ;;
	esac
	cp "$f" /out/lib32/
	so=$(readelf -d "$f" | sed -n 's/.*(SONAME).*\[\(.*\)\]/\1/p')
	[ -z "$so" ] || [ "$so" = "$b" ] || ln -sf "$b" "/out/lib32/$so"
done
mkdir -p /out/lib32/nvidia/gbm
[ -e /out/lib32/libnvidia-allocator.so.1 ]
ln -s ../../libnvidia-allocator.so.1 /out/lib32/nvidia/gbm/nvidia-drm_gbm.so
# The new list is shorter; NUL pads it, and nothing in the file moves.
old=/etc/egl/egl_external_platform.d:/usr/share/egl/egl_external_platform.d
new=/etc/egl/egl_external_platform.d:/lib/nvidia/egl_external_platform.d
for e in /out/lib/libEGL_nvidia.so.$VERSION /out/lib32/libEGL_nvidia.so.$VERSION; do
	off=$(grep -obUaF "$old" "$e" | head -n 1 | cut -d: -f1)
	[ -n "$off" ]
	{
		printf '%s' "$new"
		i=${#new}
		while [ "$i" -lt "${#old}" ]; do printf '\0'; i=$((i + 1)); done
	} | dd of="$e" bs=1 seek="$off" conv=notrunc status=none
	grep -qaF "$new" "$e"
done

s=/out/share
mkdir -p "$s/vulkan/icd.d" "$s/vulkan/implicit_layer.d" "$s/glvnd/egl_vendor.d"
cp nvidia_icd.json "$s/vulkan/icd.d/"
cp nvidia_layers.json "$s/vulkan/implicit_layer.d/"
cp 10_nvidia.json "$s/glvnd/egl_vendor.d/"

cp nvidia-smi /out/bin/
off=$(grep -obUaF /lib64/ld-linux-x86-64.so.2 /out/bin/nvidia-smi | head -n 1 | cut -d: -f1)
[ "$off" -lt 4096 ]
printf '/lib/ld-linux-x86-64.so.2\0\0\0' | dd of=/out/bin/nvidia-smi bs=1 seek="$off" conv=notrunc status=none

mkdir -p "/out/lib/firmware/nvidia/$VERSION"
cp firmware/*.bin "/out/lib/firmware/nvidia/$VERSION/"
