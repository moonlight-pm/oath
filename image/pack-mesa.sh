#!/bin/bash
# Pack/live-install pkg:mesa from Debian mesa 26.2.1-4. Same layout as
# image/install-arcade-runtime.sh (64-bit GLX + RADV, 32-bit RADV).
# Does not rebuild gamescope/Steam.
set -euo pipefail

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
fetchdir=${OATH_FETCH:-$root/build/fetch}
stagedir=${OATH_STAGE:-$root/build/mesa-stage}
store=/oath/store/pkg
objects=/oath/objects/pkg
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
river=/oath/store/pkg/river/lib
zstd=${ZSTD:-/tmp/zstd}

mkdir -p "$fetchdir" "$stagedir" "$stagedir/debroot" "$stagedir/debroot32"

as_root() {
	if [ "$(id -u)" = 0 ]; then "$@"; else sudo -n "$@"; fi
}

is_elf() {
	[ -f "$1" ] || return 1
	[ "$(head -c 4 "$1" 2>/dev/null)" = $'\x7fELF' ]
}

extract_deb() {
	local deb=$1 dest=$2 tmp
	tmp=$(mktemp -d)
	(cd "$tmp" && /oath/store/pkg/cc/libexec/zig/zig ar x "$deb")
	mkdir -p "$dest"
	if [ -f "$tmp/data.tar.zst" ]; then
		"$zstd" -d "$tmp/data.tar.zst" -c | tar -x -C "$dest"
	elif [ -f "$tmp/data.tar.xz" ]; then
		tar -xJf "$tmp/data.tar.xz" -C "$dest"
	elif [ -f "$tmp/data.tar.gz" ]; then
		tar -xzf "$tmp/data.tar.gz" -C "$dest"
	elif [ -f "$tmp/data.tar" ]; then
		tar -xf "$tmp/data.tar" -C "$dest"
	else
		echo "extract_deb: no data.tar in $deb" >&2
		ls "$tmp" >&2
		rm -rf "$tmp"
		return 1
	fi
	rm -rf "$tmp"
}

debian_mirror=https://deb.debian.org/debian/pool/main
fetch_debian() {
	local rel=$1 dest=$fetchdir/$(basename "$rel")
	local url=$debian_mirror/$(printf '%s' "$rel" | sed 's/+/%2B/g')
	if [ -f "$dest" ] && [ -s "$dest" ]; then
		echo "cached $dest"
		return 0
	fi
	echo "fetch $url"
	curl -fL --retry 3 --retry-delay 2 -o "$dest" "$url"
}

echo "==> pack mesa 26.2.1-4"
for rel in \
	m/mesa/libglx-mesa0_26.2.1-4_amd64.deb \
	m/mesa/mesa-libgallium_26.2.1-4_amd64.deb \
	m/mesa/libgl1-mesa-dri_26.2.1-4_amd64.deb \
	m/mesa/libgbm1_26.2.1-4_amd64.deb \
	m/mesa/mesa-vulkan-drivers_26.2.1-4_amd64.deb \
	libg/libglvnd/libgl1_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglx0_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglvnd0_1.7.0-3+b1_amd64.deb \
	libx/libxcb/libxcb-glx0_1.17.0-2+b2_amd64.deb \
	libd/libdrm/libdrm-common_2.4.134-3_all.deb \
	libd/libdrm/libdrm2_2.4.134-3_amd64.deb \
	libd/libdrm/libdrm-amdgpu1_2.4.134-3_amd64.deb \
	v/vulkan-loader/libvulkan1_1.4.357.0-1_amd64.deb \
	v/vulkan-tools/vulkan-tools_1.4.341.0+dfsg1-1_amd64.deb
do
	fetch_debian "$rel"
	extract_deb "$fetchdir/$(basename "$rel")" "$stagedir/debroot"
done

rm -rf "$stagedir/mesa"
mkdir -p "$stagedir/mesa/lib/dri" "$stagedir/mesa/lib/gbm" "$stagedir/mesa/share/libdrm" "$stagedir/mesa/share/glvnd"
mesa_src=$stagedir/debroot/usr/lib/x86_64-linux-gnu
mesa_rpath="$glibc:/oath/store/pkg/mesa/lib:$river:/oath/store/pkg/xwayland/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/sola/lib"
copy_mesa() {
	local src=$1 dest=$2
	[ -e "$src" ] || { echo "missing $src" >&2; return 1; }
	cp -aL "$src" "$dest"
	chmod u+w "$dest" 2>/dev/null || true
}
copy_mesa "$mesa_src/libGL.so.1.7.0" "$stagedir/mesa/lib/libGL.so.1.7.0"
copy_mesa "$mesa_src/libGLX.so.0.0.0" "$stagedir/mesa/lib/libGLX.so.0.0.0"
copy_mesa "$mesa_src/libGLdispatch.so.0.0.0" "$stagedir/mesa/lib/libGLdispatch.so.0.0.0"
copy_mesa "$mesa_src/libGLX_mesa.so.0.0.0" "$stagedir/mesa/lib/libGLX_mesa.so.0.0.0"
gallium_so=$(find "$mesa_src" -maxdepth 1 -name 'libgallium-*.so' ! -type l | head -1)
[ -n "$gallium_so" ] || { echo "missing libgallium in $mesa_src" >&2; exit 1; }
copy_mesa "$gallium_so" "$stagedir/mesa/lib/$(basename "$gallium_so")"
# Mesa 26.2.1 RADV needs libdrm >= 2.4.134 (amdgpu_sw_info_address_prt_wa_control_bit).
# River's libdrm only implements address32_hi and RADV treats the miss as fatal.
drm_so=$(find "$stagedir/debroot" -name 'libdrm.so.2.134.0' ! -type l | head -1)
drm_amd=$(find "$stagedir/debroot" -name 'libdrm_amdgpu.so.1.134.0' ! -type l | head -1)
[ -n "$drm_so" ] || { echo "missing libdrm.so.2.134.0" >&2; exit 1; }
[ -n "$drm_amd" ] || { echo "missing libdrm_amdgpu.so.1.134.0" >&2; exit 1; }
copy_mesa "$drm_so" "$stagedir/mesa/lib/libdrm.so.2.134.0"
copy_mesa "$drm_amd" "$stagedir/mesa/lib/libdrm_amdgpu.so.1.134.0"
ln -sfn libdrm.so.2.134.0 "$stagedir/mesa/lib/libdrm.so.2"
ln -sfn libdrm_amdgpu.so.1.134.0 "$stagedir/mesa/lib/libdrm_amdgpu.so.1"
copy_mesa "$mesa_src/libgbm.so.1.0.0" "$stagedir/mesa/lib/libgbm.so.1.0.0"
copy_mesa "$mesa_src/libxcb-glx.so.0" "$stagedir/mesa/lib/libxcb-glx.so.0" || \
	copy_mesa "$(find "$stagedir/debroot" -name 'libxcb-glx.so.0*' ! -type l | head -1)" "$stagedir/mesa/lib/libxcb-glx.so.0"
copy_mesa "$mesa_src/dri/libdril_dri.so" "$stagedir/mesa/lib/dri/libdril_dri.so"
copy_mesa "$mesa_src/gbm/dri_gbm.so" "$stagedir/mesa/lib/gbm/dri_gbm.so"
if [ -f "$stagedir/debroot/usr/share/libdrm/amdgpu.ids" ]; then
	copy_mesa "$stagedir/debroot/usr/share/libdrm/amdgpu.ids" "$stagedir/mesa/share/libdrm/amdgpu.ids"
fi
ln -sfn libGL.so.1.7.0 "$stagedir/mesa/lib/libGL.so.1"
ln -sfn libGLX.so.0.0.0 "$stagedir/mesa/lib/libGLX.so.0"
ln -sfn libGLdispatch.so.0.0.0 "$stagedir/mesa/lib/libGLdispatch.so.0"
ln -sfn libGLX_mesa.so.0.0.0 "$stagedir/mesa/lib/libGLX_mesa.so.0"
ln -sfn libGLX_mesa.so.0 "$stagedir/mesa/lib/libGLX_indirect.so.0"
ln -sfn libgbm.so.1.0.0 "$stagedir/mesa/lib/libgbm.so.1"
ln -sfn libdril_dri.so "$stagedir/mesa/lib/dri/radeonsi_dri.so"
ln -sfn libdril_dri.so "$stagedir/mesa/lib/dri/swrast_dri.so"
ln -sfn libdril_dri.so "$stagedir/mesa/lib/dri/kms_swrast_dri.so"
cat >"$stagedir/mesa/share/glvnd/10_mesa.json" <<'JSON'
{
    "file_format_version" : "1.0.0",
    "ICD" : {
        "library_path" : "/oath/store/pkg/mesa/lib/libGLX_mesa.so.0"
    }
}
JSON
find "$stagedir/mesa/lib" "$stagedir/mesa/lib/dri" "$stagedir/mesa/lib/gbm" -type f | while read -r f; do
	is_elf "$f" || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp" "$f" || true
	fi
	patchelf --set-rpath "$mesa_rpath" "$f" 2>/dev/null || true
done
for f in "$stagedir/mesa/lib"/lib*.so*; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	so=$(patchelf --print-soname "$f" 2>/dev/null || true)
	[ -n "$so" ] || continue
	if [ "$so" != "$(basename "$f")" ]; then
		ln -sfn "$(basename "$f")" "$stagedir/mesa/lib/$so"
	fi
done
vk_so=$(find "$stagedir/debroot" -name 'libvulkan.so.1.*' ! -type l | head -1)
radeon_so=$(find "$stagedir/debroot" -name 'libvulkan_radeon.so' ! -type l | head -1)
virtio_so=$(find "$stagedir/debroot" -name 'libvulkan_virtio.so' ! -type l | head -1)
vinfo=$(find "$stagedir/debroot" -type f -name vulkaninfo | head -1)
[ -n "$vk_so" ] || { echo "missing libvulkan.so.1" >&2; exit 1; }
[ -n "$radeon_so" ] || { echo "missing libvulkan_radeon.so" >&2; exit 1; }
mkdir -p "$stagedir/mesa/libexec" "$stagedir/mesa/bin" "$stagedir/mesa/share/vulkan/icd.d"
copy_mesa "$vk_so" "$stagedir/mesa/lib/$(basename "$vk_so")"
copy_mesa "$radeon_so" "$stagedir/mesa/lib/libvulkan_radeon.so"
if [ -n "$virtio_so" ]; then
	copy_mesa "$virtio_so" "$stagedir/mesa/lib/libvulkan_virtio.so"
fi
copy_mesa "$vinfo" "$stagedir/mesa/libexec/vulkaninfo"
ln -sfn "$(basename "$vk_so")" "$stagedir/mesa/lib/libvulkan.so.1"
cat >"$stagedir/mesa/share/vulkan/icd.d/radeon_icd.json" <<'JSON'
{
    "ICD": {
        "api_version": "1.4.354",
        "library_path": "/oath/store/pkg/mesa/lib/libvulkan_radeon.so"
    },
    "file_format_version": "1.0.1"
}
JSON
if [ -f "$stagedir/mesa/lib/libvulkan_virtio.so" ]; then
	cat >"$stagedir/mesa/share/vulkan/icd.d/virtio_icd.json" <<'JSON'
{
    "ICD": {
        "api_version": "1.4.354",
        "library_path": "/oath/store/pkg/mesa/lib/libvulkan_virtio.so"
    },
    "file_format_version": "1.0.1"
}
JSON
fi
for f in "$stagedir/mesa/lib"/libvulkan.so.1.* "$stagedir/mesa/lib/libvulkan_radeon.so" \
	"$stagedir/mesa/lib/libvulkan_virtio.so" "$stagedir/mesa/libexec/vulkaninfo"; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp" "$f" || true
	fi
	patchelf --set-rpath "$mesa_rpath" "$f" 2>/dev/null || true
done
cat >"$stagedir/mesa/bin/vulkaninfo" <<'WRAP'
#!/bin/sh
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json}"
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
export DISABLE_LAYER_MESA_DEVICE_SELECT=1
export LD_LIBRARY_PATH="/oath/store/pkg/mesa/lib:/oath/store/pkg/river/lib:/oath/store/pkg/xwayland/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/glibc/lib:${LD_LIBRARY_PATH:-}"
exec /oath/store/pkg/mesa/libexec/vulkaninfo "$@"
WRAP
chmod 755 "$stagedir/mesa/bin/vulkaninfo"

fetch_debian m/mesa/mesa-vulkan-drivers_26.2.1-4_i386.deb
# Cached i386 names from the T37 pack (Debian + in the version became _).
extract_deb "$fetchdir/mesa-vulkan-drivers_26.2.1-4_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libllvm21_21.1.8-10_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libvulkan1_1.4.357.0-1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libdisplay-info3_0.3.0-1_b1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libxml2-16_2.15.3_dfsg-1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libwayland-client0_1.26.0-1_i386.deb" "$stagedir/debroot32"
fetch_debian libd/libdrm/libdrm2_2.4.134-3_i386.deb
fetch_debian libd/libdrm/libdrm-amdgpu1_2.4.134-3_i386.deb
fetch_debian m/mesa/libgbm1_26.2.1-4_i386.deb
extract_deb "$fetchdir/libdrm2_2.4.134-3_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libdrm-amdgpu1_2.4.134-3_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libgbm1_26.2.1-4_i386.deb" "$stagedir/debroot32"
mkdir -p "$stagedir/mesa/lib32"
radeon32=$(find "$stagedir/debroot32" -name 'libvulkan_radeon.so' ! -type l | head -1)
llvm32=$(find "$stagedir/debroot32" -name 'libLLVM.so.21.1' ! -type l | head -1)
vk32=$(find "$stagedir/debroot32" -name 'libvulkan.so.1.*' ! -type l | head -1)
di32=$(find "$stagedir/debroot32" -name 'libdisplay-info.so.0.3.0' ! -type l | head -1)
xml32=$(find "$stagedir/debroot32" -name 'libxml2.so.16.*' ! -type l | head -1)
wl32=$(find "$stagedir/debroot32" -name 'libwayland-client.so.0.*' ! -type l | head -1)
copy_mesa "$radeon32" "$stagedir/mesa/lib32/libvulkan_radeon.so"
copy_mesa "$llvm32" "$stagedir/mesa/lib32/libLLVM.so.21.1"
copy_mesa "$vk32" "$stagedir/mesa/lib32/$(basename "$vk32")"
ln -sfn "$(basename "$vk32")" "$stagedir/mesa/lib32/libvulkan.so.1"
[ -n "$di32" ] || { echo "missing 32-bit libdisplay-info" >&2; exit 1; }
[ -n "$xml32" ] || { echo "missing 32-bit libxml2.so.16" >&2; exit 1; }
[ -n "$wl32" ] || { echo "missing 32-bit libwayland-client" >&2; exit 1; }
copy_mesa "$di32" "$stagedir/mesa/lib32/$(basename "$di32")"
copy_mesa "$xml32" "$stagedir/mesa/lib32/$(basename "$xml32")"
copy_mesa "$wl32" "$stagedir/mesa/lib32/$(basename "$wl32")"
drm32=$(find "$stagedir/debroot32" -name 'libdrm.so.2.134.0' ! -type l | head -1)
drmamd32=$(find "$stagedir/debroot32" -name 'libdrm_amdgpu.so.1.134.0' ! -type l | head -1)
[ -n "$drm32" ] || { echo "missing 32-bit libdrm.so.2.134.0" >&2; exit 1; }
[ -n "$drmamd32" ] || { echo "missing 32-bit libdrm_amdgpu.so.1.134.0" >&2; exit 1; }
copy_mesa "$drm32" "$stagedir/mesa/lib32/libdrm.so.2.134.0"
copy_mesa "$drmamd32" "$stagedir/mesa/lib32/libdrm_amdgpu.so.1.134.0"
ln -sfn libdrm.so.2.134.0 "$stagedir/mesa/lib32/libdrm.so.2"
ln -sfn libdrm_amdgpu.so.1.134.0 "$stagedir/mesa/lib32/libdrm_amdgpu.so.1"
gbm32=$(find "$stagedir/debroot32" -name 'libgbm.so.1.0.0' ! -type l | head -1)
if [ -n "$gbm32" ]; then
	copy_mesa "$gbm32" "$stagedir/mesa/lib32/libgbm.so.1.0.0"
	ln -sfn libgbm.so.1.0.0 "$stagedir/mesa/lib32/libgbm.so.1"
fi
ln -sfn "$(basename "$di32")" "$stagedir/mesa/lib32/libdisplay-info.so.3"
ln -sfn "$(basename "$xml32")" "$stagedir/mesa/lib32/libxml2.so.16"
ln -sfn "$(basename "$wl32")" "$stagedir/mesa/lib32/libwayland-client.so.0"
interp32=/oath/store/pkg/steam/lib32/ld-linux.so.2
rpath32="/oath/store/pkg/mesa/lib32:/oath/store/pkg/steam/lib32:$glibc"
for f in "$stagedir/mesa/lib32"/*; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	is_elf "$f" || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp32" "$f" || true
	fi
	patchelf --set-rpath "$rpath32" "$f" 2>/dev/null || true
done
patchelf --force-rpath --set-rpath "$rpath32" "$stagedir/mesa/lib32/libvulkan_radeon.so"
patchelf --force-rpath --set-rpath "$mesa_rpath" "$stagedir/mesa/lib/libvulkan_radeon.so"
cat >"$stagedir/mesa/share/vulkan/icd.d/radeon_icd32.json" <<'JSON'
{
    "ICD": {
        "api_version": "1.4.354",
        "library_path": "/oath/store/pkg/mesa/lib32/libvulkan_radeon.so"
    },
    "file_format_version": "1.0.1"
}
JSON
cat >"$stagedir/mesa/INDEX.md" <<'EOF'
# pkg:mesa

64-bit OpenGL/GLX and Vulkan WSI for X11/Wayland clients. Debian mesa
26.2.1 GLX + glvnd + gallium + RADV, plus Khronos vulkan-loader 1.4.357
and vulkaninfo. DRI is libdril → radeonsi. ICD is
share/vulkan/icd.d/radeon_icd.json (64-bit) and radeon_icd32.json
(32-bit Steam). lib32 ships RADV + LLVM 21 + libdisplay-info.so.3 +
libxml2.so.16 + libwayland-client 1.26 (`wl_fixes_interface`; steamrt
0.3.0 is too old) + libgbm.so.1. ICD uses DT_RPATH so Steam's steamrt
LD_LIBRARY_PATH cannot hide the new wayland. 64-bit LLVM stays in
pkg:river. Ships Debian libdrm 2.4.134 in lib/ and lib32/ so RADV's
amdgpu_sw_info_address_prt_wa_control_bit query succeeds (river's
libdrm only implements address32_hi). Removable.
EOF

echo "==> install pkg:mesa"
as_root rm -rf "$store/mesa"
as_root mkdir -p "$store"
as_root cp -a "$stagedir/mesa" "$store/mesa"
as_root chmod -R u+rX "$store/mesa"
as_root mkdir -p "$objects/mesa"
printf '%s\n' '{ "present": true }' | as_root tee "$objects/mesa/desired.json" >/dev/null
if [ "$(id -u)" = 0 ]; then
	oath apply pkg:mesa
else
	sudo -n oath apply pkg:mesa
fi
ls -l /oath/store/pkg/mesa/lib/libgallium-*.so
echo "mesa 26.2.1 installed"
