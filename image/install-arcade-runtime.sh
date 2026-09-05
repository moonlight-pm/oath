#!/bin/bash
# Live-install pkg:xwayland, pkg:gamescope, pkg:mesa, pkg:steam and
# sola-arcade on this Oath box. Ubuntu questing debs + Steam bootstrap
# + Debian i386 libc + Debian mesa 26.1.6 GLX.
# Busybox dpkg-deb cannot unpack zstd debs; use /tmp/zstd from image/pack.
set -euo pipefail

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
fetchdir=${OATH_FETCH:-$root/build/fetch}
stagedir=${OATH_STAGE:-$root/build/arcade-stage}
store=/oath/store/pkg
objects=/oath/objects/pkg
mirror=http://archive.ubuntu.com/ubuntu
zstd=${ZSTD:-/tmp/zstd}
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
river=/oath/store/pkg/river/lib
pw=/oath/store/pkg/pipewire/lib

mkdir -p "$fetchdir" "$stagedir" "$stagedir/debroot"

as_root() {
	if [ "$(id -u)" = 0 ]; then "$@"; else sudo -n "$@"; fi
}

is_elf() {
	[ -f "$1" ] || return 1
	[ "$(head -c 4 "$1" 2>/dev/null)" = $'\x7fELF' ]
}

extract_deb() {
	local deb=$1 dest=$2
	local tmp
	tmp=$(mktemp -d)
	(cd "$tmp" && ar x "$deb")
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

fetch_deb() {
	local rel=$1
	local dest=$fetchdir/$(basename "$rel")
	if [ -f "$dest" ] && [ -s "$dest" ]; then
		echo "cached $dest"
		return 0
	fi
	echo "fetch $mirror/$rel"
	curl -fL --retry 3 --retry-delay 2 -o "$dest" "$mirror/$rel"
}

write_obj() {
	local name=$1
	local dir=$objects/$name
	as_root mkdir -p "$dir"
	printf '%s\n' '{ "present": true }' | as_root tee "$dir/desired.json" >/dev/null
	printf '%s\n' '{ "present": false, "links": [], "removable": true }' | as_root tee "$dir/actual.json" >/dev/null
	printf '%s\n' "{ \"id\": \"pkg:$name\", \"kind\": \"pkg\", \"name\": \"$name\", \"safety\": \"mutate\", \"status\": \"drift\" }" | as_root tee "$dir/meta.json" >/dev/null
}

install_store() {
	local name=$1 src=$2
	echo "==> install pkg:$name"
	as_root rm -rf "$store/$name"
	as_root mkdir -p "$store"
	as_root cp -a "$src" "$store/$name"
	as_root chmod -R u+rX "$store/$name"
	write_obj "$name"
}

# --- zstd ---
if [ ! -x "$zstd" ]; then
	echo "need zstd at $zstd (compile image/pack zstd first)" >&2
	exit 1
fi

# --- Ubuntu debs (questing) ---
debs=(
	pool/main/libx/libxdamage/libxdamage1_1.1.6-1build1_amd64.deb
	pool/main/libx/libxfixes/libxfixes3_6.0.0-2build1_amd64.deb
	pool/main/libx/libxcomposite/libxcomposite1_0.4.6-1_amd64.deb
	pool/main/libx/libxrender/libxrender1_0.9.12-1_amd64.deb
	pool/main/libx/libxext/libxext6_1.3.4-1build2_amd64.deb
	pool/main/libx/libxxf86vm/libxxf86vm1_1.1.4-1build4_amd64.deb
	pool/main/libx/libxres/libxres1_1.2.1-1build1_amd64.deb
	pool/main/libx/libxtst/libxtst6_1.2.5-1_amd64.deb
	pool/main/libx/libxmu/libxmu6_1.1.3-3build3_amd64.deb
	pool/main/libx/libxcursor/libxcursor1_1.2.3-1_amd64.deb
	pool/main/libx/libxi/libxi6_1.8.2-1_amd64.deb
	pool/main/libs/libsdl2/libsdl2-2.0-0_2.32.4+dfsg-1_amd64.deb
	pool/main/libc/libcap2/libcap2_2.75-7ubuntu2_amd64.deb
	pool/universe/liba/libavif/libavif16_1.3.0-1ubuntu1_amd64.deb
	pool/main/libd/libdecor-0/libdecor-0-0_0.2.2-2_amd64.deb
	pool/main/libe/libei/libeis1_1.3.901-1_amd64.deb
	pool/universe/l/luajit/libluajit-5.1-2_2.1.0+openresty20250117-2ubuntu1_amd64.deb
	pool/main/libx/libx11/libx11-6_1.8.12-1build1_amd64.deb
	pool/main/libx/libxau/libxau6_1.0.11-1build1_amd64.deb
	pool/main/libx/libxdmcp/libxdmcp6_1.1.5-1build1_amd64.deb
	pool/main/libx/libxcb/libxcb1_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-render0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-shm0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-xfixes0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-randr0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-shape0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-sync1_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-xkb1_1.17.0-2build1_amd64.deb
	pool/universe/x/xcb-util-image/libxcb-image0_0.4.0-2build1_amd64.deb
	pool/main/x/xcb-util/libxcb-util1_0.4.1-1_amd64.deb
	pool/universe/x/xcb-util-wm/libxcb-icccm4_0.4.2-1_amd64.deb
	pool/universe/x/xcb-util-keysyms/libxcb-keysyms1_0.4.1-1_amd64.deb
	pool/main/libx/libxcb/libxcb-present0_1.17.0-2build1_amd64.deb
	pool/main/libx/libxcb/libxcb-dri3-0_1.17.0-2build1_amd64.deb
	pool/universe/x/xcb-util-wm/libxcb-ewmh2_0.4.2-1_amd64.deb
	pool/main/libs/libsm/libsm6_1.2.6-1_amd64.deb
	pool/main/libi/libice/libice6_1.1.1-1_amd64.deb
	pool/main/libb/libbsd/libbsd0_0.12.2-2build1_amd64.deb
	pool/main/libm/libmd/libmd0_1.1.0-2build3_amd64.deb
	pool/main/libf/libffi/libffi8_3.5.2-1build1_amd64.deb
	pool/main/libe/libepoxy/libepoxy0_1.5.10-2_amd64.deb
	pool/multiverse/o/openvr/libopenvr-api1t64_1.23.7~ds1-2.1build2_amd64.deb
	pool/main/libx/libxt/libxt6t64_1.2.1-1.3_amd64.deb
	pool/main/libx/libxfont/libxfont2_2.0.6-1build1_amd64.deb
	pool/main/libx/libxcvt/libxcvt0_0.1.3-1_amd64.deb
	pool/main/libe/libei/liboeffis1_1.3.901-1_amd64.deb
	pool/main/libt/libtirpc/libtirpc3t64_1.3.6+ds-1_amd64.deb
	pool/main/libg/libgcrypt20/libgcrypt20_1.11.0-7build1_amd64.deb
	pool/main/libg/libgpg-error/libgpg-error0_1.51-4_amd64.deb
	pool/main/libe/libei/libei1_1.3.901-1_amd64.deb
)

echo "==> fetch ubuntu libs"
for rel in "${debs[@]}"; do
	fetch_deb "$rel"
	extract_deb "$fetchdir/$(basename "$rel")" "$stagedir/debroot"
done

# --- relocate gamescope + xwayland ---
relocate_bins() {
	local out=$1
	shift
	local guest_lib=/oath/store/pkg/$(basename "$out")/lib
	local rpath="$glibc:$guest_lib:$river:$pw"
	rm -rf "$out"
	mkdir -p "$out/bin" "$out/lib" "$out/libexec" "$out/share"
	declare -A SEEN=()
	queue=()
	enqueue() {
		local f=$1
		[[ -e $f ]] || return 0
		local real
		real=$(readlink -f "$f")
		[[ -n ${SEEN[$real]+x} ]] && return 0
		SEEN[$real]=1
		queue+=("$real")
	}
	local b
	for b in "$@"; do
		[[ -f $b ]] || { echo "missing $b" >&2; exit 1; }
		cp -a "$b" "$out/libexec/$(basename "$b")"
		chmod u+w "$out/libexec/$(basename "$b")"
		enqueue "$out/libexec/$(basename "$b")"
	done
	# Staging prefix libs first so NEEDED resolves.
	export LD_LIBRARY_PATH="$stagedir/debroot/usr/lib/x86_64-linux-gnu:$stagedir/debroot/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"
	local i=0 f loader=""
	while [[ $i -lt ${#queue[@]} ]]; do
		f=${queue[$i]}
		i=$((i + 1))
		is_elf "$f" || continue
		if [[ -z $loader ]]; then
			loader=$(patchelf --print-interpreter "$f" 2>/dev/null || true)
			[[ -n $loader && -e $loader ]] && enqueue "$loader"
			loader=$interp
		fi
		local deps
		deps=$(mktemp)
		"$interp" --library-path "$glibc:$river:$pw:$stagedir/debroot/usr/lib/x86_64-linux-gnu:$stagedir/debroot/lib/x86_64-linux-gnu" --list "$f" 2>/dev/null | awk '/=> \// {print $3} /^\//{print $1}' >"$deps" || true
		while read -r dep; do
			[[ -e $dep ]] && enqueue "$dep"
		done <"$deps"
		rm -f "$deps"
	done
	local name d
	for f in "${!SEEN[@]}"; do
		name=$(basename "$f")
		case $name in
		ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libgcc_s.so*|libstdc++.so*)
			continue ;;
		esac
		[[ $f == "$out"/* ]] && continue
		d="$out/lib/$name"
		cp -a "$f" "$d"
		chmod u+w "$d" 2>/dev/null || true
	done
	# SONAME links
	for f in "$out"/lib/*; do
		[[ -f $f && ! -L $f ]] || continue
		local soname
		soname=$(patchelf --print-soname "$f" 2>/dev/null || true)
		[[ -n $soname ]] || continue
		if [[ $soname != "$(basename "$f")" ]]; then
			ln -sfn "$(basename "$f")" "$out/lib/$soname"
		fi
	done
	find "$out/lib" "$out/libexec" -type f | while read -r f; do
		is_elf "$f" || continue
		chmod u+w "$f" || true
		if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
			patchelf --set-interpreter "$interp" "$f" || true
		fi
		patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
	done
}

echo "==> pack gamescope"
gs_src=$stagedir/debroot/usr/games
if [ ! -f /tmp/gs/usr/games/gamescope ]; then
	extract_deb "$fetchdir/gamescope_3.16.24+ds-2_amd64.deb" "$stagedir/debroot"
fi
# Prefer already-extracted /tmp/gs if present.
gs_bin=${GAMESCOPE_BIN:-}
if [ -x /tmp/gs/usr/games/gamescope ]; then
	gs_bin=/tmp/gs/usr/games
else
	gs_bin=$stagedir/debroot/usr/games
fi
relocate_bins "$stagedir/gamescope" \
	"$gs_bin/gamescope" "$gs_bin/gamescopectl" "$gs_bin/gamescopereaper"
if [ -d /tmp/gs/usr/share/gamescope ]; then
	cp -a /tmp/gs/usr/share/gamescope "$stagedir/gamescope/share/gamescope"
elif [ -d "$stagedir/debroot/usr/share/gamescope" ]; then
	cp -a "$stagedir/debroot/usr/share/gamescope" "$stagedir/gamescope/share/gamescope"
fi
if [ -d /tmp/gs/usr/lib/x86_64-linux-gnu/gamescope ]; then
	cp -a /tmp/gs/usr/lib/x86_64-linux-gnu/gamescope/. "$stagedir/gamescope/lib/"
fi
cat >"$stagedir/gamescope/bin/gamescope" <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export VK_ICD_FILENAMES=/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
export DISABLE_LAYER_MESA_DEVICE_SELECT=1
export NODEVICE_SELECT=1
# libmvec.so.1 is a real glibc object (GLIBC_2.22). Do not let a
# libmvec→libm symlink win; that is "GLIBC_2.22 not found".
export LD_LIBRARY_PATH="/oath/store/pkg/mesa/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/xwayland/lib:/oath/store/pkg/pipewire/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib:/oath/store/pkg/sola/lib"
exec /oath/store/pkg/gamescope/libexec/gamescope "$@"
WRAP
chmod 755 "$stagedir/gamescope/bin/gamescope"
cat >"$stagedir/gamescope/INDEX.md" <<'EOF'
# pkg:gamescope

Windowed nest compositor for sola-arcade. Ubuntu questing 3.16 gamescope
relocated onto pkg:glibc + pkg:river. Removable. PID 1 does not supervise it.
EOF

echo "==> pack xwayland"
extract_deb "$fetchdir/xwayland_24.1.13-1_amd64.deb" "$stagedir/debroot"
relocate_bins "$stagedir/xwayland" "$stagedir/debroot/usr/bin/Xwayland"
cat >"$stagedir/xwayland/bin/Xwayland" <<'WRAP'
#!/bin/sh
export PATH=/bin:/usr/bin
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export XKB_CONFIG_ROOT="${XKB_CONFIG_ROOT:-/oath/store/pkg/river/share/X11/xkb}"
export XKB_BINDIR=/oath/store/pkg/xwayland/libexec
export LD_LIBRARY_PATH="/oath/store/pkg/xwayland/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib:/oath/store/pkg/sola/lib"
exec /oath/store/pkg/xwayland/libexec/Xwayland "$@"
WRAP
chmod 755 "$stagedir/xwayland/bin/Xwayland"
# gamescope looks up Xwayland on PATH
ln -sfn /oath/store/pkg/xwayland/bin/Xwayland "$stagedir/gamescope/bin/Xwayland" 2>/dev/null || true
cat >"$stagedir/xwayland/INDEX.md" <<'EOF'
# pkg:xwayland

Xwayland for gamescope's nested X and (later) host River. Debian 24.1
relocated onto pkg:glibc + pkg:river. Removable.
EOF

echo "==> pack mesa (64-bit GLX)"
debian_mirror=https://deb.debian.org/debian/pool/main
fetch_debian() {
	local rel=$1
	local dest=$fetchdir/$(basename "$rel")
	if [ -f "$dest" ] && [ -s "$dest" ]; then
		echo "cached $dest"
		return 0
	fi
	echo "fetch $debian_mirror/$rel"
	curl -fL --retry 3 --retry-delay 2 -o "$dest" "$debian_mirror/$rel"
}
for rel in \
	m/mesa/libglx-mesa0_26.1.6-1_amd64.deb \
	m/mesa/mesa-libgallium_26.1.6-1_amd64.deb \
	m/mesa/libgl1-mesa-dri_26.1.6-1_amd64.deb \
	m/mesa/libgbm1_26.1.6-1_amd64.deb \
	m/mesa/mesa-vulkan-drivers_26.1.6-1_amd64.deb \
	libg/libglvnd/libgl1_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglx0_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglvnd0_1.7.0-3+b1_amd64.deb \
	libx/libxcb/libxcb-glx0_1.17.0-2+b2_amd64.deb \
	libd/libdrm/libdrm-common_2.4.124-2_all.deb \
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
# glvnd dispatch + mesa GLX vendor + matching gallium (not river 26.1.0).
copy_mesa "$mesa_src/libGL.so.1.7.0" "$stagedir/mesa/lib/libGL.so.1.7.0"
copy_mesa "$mesa_src/libGLX.so.0.0.0" "$stagedir/mesa/lib/libGLX.so.0.0.0"
copy_mesa "$mesa_src/libGLdispatch.so.0.0.0" "$stagedir/mesa/lib/libGLdispatch.so.0.0.0"
copy_mesa "$mesa_src/libGLX_mesa.so.0.0.0" "$stagedir/mesa/lib/libGLX_mesa.so.0.0.0"
copy_mesa "$mesa_src/libgallium-26.1.6-1.so" "$stagedir/mesa/lib/libgallium-26.1.6-1.so"
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
# glvnd GLX vendor file (absolute path so CEF does not search /usr).
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
# SONAME links for versioned copies (xcb-glx may be libxcb-glx.so.0.0.0).
for f in "$stagedir/mesa/lib"/lib*.so*; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	so=$(patchelf --print-soname "$f" 2>/dev/null || true)
	[ -n "$so" ] || continue
	if [ "$so" != "$(basename "$f")" ]; then
		ln -sfn "$(basename "$f")" "$stagedir/mesa/lib/$so"
	fi
done
# Vulkan loader + RADV/virtio ICDs (WSI: wayland/xlib/xcb) + vulkaninfo.
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
# 32-bit RADV so the ubuntu12_32 Steam client can vkCreateInstance
# (64-bit ICD json is the default for gamescope/vulkaninfo/pv-host).
fetch_debian m/mesa/mesa-vulkan-drivers_26.1.6-1_i386.deb
fetch_debian l/llvm-toolchain-21/libllvm21_21.1.8-10_i386.deb
fetch_debian v/vulkan-loader/libvulkan1_1.4.357.0-1_i386.deb
extract_deb "$fetchdir/mesa-vulkan-drivers_26.1.6-1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libllvm21_21.1.8-10_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libvulkan1_1.4.357.0-1_i386.deb" "$stagedir/debroot32"
mkdir -p "$stagedir/mesa/lib32"
radeon32=$(find "$stagedir/debroot32" -name 'libvulkan_radeon.so' ! -type l | head -1)
llvm32=$(find "$stagedir/debroot32" -name 'libLLVM.so.21.1' ! -type l | head -1)
vk32=$(find "$stagedir/debroot32" -name 'libvulkan.so.1.*' ! -type l | head -1)
copy_mesa "$radeon32" "$stagedir/mesa/lib32/libvulkan_radeon.so"
copy_mesa "$llvm32" "$stagedir/mesa/lib32/libLLVM.so.21.1"
copy_mesa "$vk32" "$stagedir/mesa/lib32/$(basename "$vk32")"
ln -sfn "$(basename "$vk32")" "$stagedir/mesa/lib32/libvulkan.so.1"
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
26.1.6 GLX + glvnd + gallium + RADV, plus Khronos vulkan-loader 1.4.357
and vulkaninfo. DRI is libdril → radeonsi. ICD is
share/vulkan/icd.d/radeon_icd.json. LLVM stays in pkg:river. Removable.
EOF

echo "==> pack steam"
rm -rf "$stagedir/steam"
mkdir -p "$stagedir/steam/bin" "$stagedir/steam/libexec" "$stagedir/steam/lib32" "$stagedir/steam/share"
extract_deb "$fetchdir/steam_latest.deb" "$stagedir/steam-deb"
cp -a "$stagedir/steam-deb/usr/lib/steam/." "$stagedir/steam/libexec/"
# 32-bit glibc (Debian sid i386). Interpreter path is /lib/ld-linux.so.2.
i386_deb=$fetchdir/libc6_i386.deb
if [ ! -s "$i386_deb" ]; then
	echo "==> fetch debian libc6 i386"
	# Try current sid names; first hit wins.
	for v in '2.41-12+deb13u4' 2.44-1 2.43-4 2.41-12; do
		enc=$(printf '%s' "$v" | sed 's/+/%2B/g')
		if curl -fL --retry 2 -o "$i386_deb" \
			"https://deb.debian.org/debian/pool/main/g/glibc/libc6_${enc}_i386.deb"; then
			break
		fi
		rm -f "$i386_deb"
	done
fi
if [ -s "$i386_deb" ]; then
	extract_deb "$i386_deb" "$stagedir/i386"
	# Debian puts the loader at lib/i386-linux-gnu/ld-linux.so.2
	find "$stagedir/i386" -name 'ld-linux.so.2' -o -name 'libc.so.6' | head
	cp -a "$stagedir/i386"/lib/i386-linux-gnu/. "$stagedir/steam/lib32/" 2>/dev/null || \
		cp -a "$stagedir/i386"/usr/lib/i386-linux-gnu/. "$stagedir/steam/lib32/" 2>/dev/null || true
	if [ -f "$stagedir/i386/lib/ld-linux.so.2" ]; then
		cp -a "$stagedir/i386/lib/ld-linux.so.2" "$stagedir/steam/lib32/ld-linux.so.2"
	fi
	find "$stagedir/i386" -name 'ld-linux.so.2' -exec cp -a {} "$stagedir/steam/lib32/ld-linux.so.2" \;
fi
cat >"$stagedir/steam/libexec/ldconfig" <<'LD'
#!/bin/sh
# Steam setup.sh runs `ldconfig -XNv`. Oath has no ld.so.cache.
if [ "$1" = "-p" ] || [ "$1" = "--print-cache" ]; then
	if [ -d /lib/i386-linux-gnu ]; then
		for f in /lib/i386-linux-gnu/*.so*; do
			[ -e "$f" ] || continue
			echo "	$(basename "$f") (libc6,x86-32) => $f"
		done
	fi
	exit 0
fi
echo "/lib/i386-linux-gnu:"
if [ -d /lib/i386-linux-gnu ]; then
	ls -1 /lib/i386-linux-gnu 2>/dev/null | sed 's/^/	/'
fi
echo "/lib64:"
echo "	ld-linux-x86-64.so.2 -> /oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2"
exit 0
LD
chmod 755 "$stagedir/steam/libexec/ldconfig"
# steam.sh get_missing_libraries calls `ldd`. No libc-bin on Oath.
cat >"$stagedir/steam/bin/ldd" <<'LDD'
#!/bin/sh
f=
while [ $# -gt 0 ]; do
	case $1 in
	--version|-v) echo "ldd (oath pkg:steam)"; exit 0 ;;
	-*) shift ;;
	*) f=$1; break ;;
	esac
done
[ -n "$f" ] && [ -e "$f" ] || { echo "ldd: missing file" >&2; exit 1; }
# ELF class at offset 4: 1=32-bit, 2=64-bit.
class=$(od -An -N1 -j4 -tu1 "$f" 2>/dev/null | tr -d ' \n')
if [ "$class" = "1" ]; then
	exec /lib/ld-linux.so.2 --list "$f"
fi
exec /lib64/ld-linux-x86-64.so.2 --list "$f"
LDD
chmod 755 "$stagedir/steam/bin/ldd"
# zenity stub — Steam only uses it for dialogs.
cat >"$stagedir/steam/bin/zenity" <<'Z'
#!/bin/sh
echo "zenity-stub: $*" >&2
exit 0
Z
chmod 755 "$stagedir/steam/bin/zenity"
# Busybox xz has no --robot; busybox tar has no --blocking-factor.
# steam.sh extract_archive needs both. Keep these off the /bin farm.
cat >"$stagedir/steam/libexec/xz" <<'XZ'
#!/bin/sh
robot= list=
for a in "$@"; do
	case "$a" in
	--robot) robot=1 ;;
	--list|-l) list=1 ;;
	esac
done
if [ -n "$robot" ] && [ -n "$list" ]; then
	# GNU xz --robot --list: steam.sh awk '{print $5}' is uncompressed bytes.
	echo "totals	1	0	1	1	0"
	exit 0
fi
exec /oath/store/pkg/busybox/bin/xz "$@"
XZ
chmod 755 "$stagedir/steam/libexec/xz"
cat >"$stagedir/steam/libexec/tar" <<'TAR'
#!/bin/sh
# Drop GNU tar flags busybox does not implement.
saved=$#
skip=
for a in "$@"; do
	if [ -n "$skip" ]; then
		skip=
		continue
	fi
	case "$a" in
	--blocking-factor|--checkpoint|--checkpoint-action) skip=1; continue ;;
	--blocking-factor=*|--checkpoint=*|--checkpoint-action=*) continue ;;
	esac
	set -- "$@" "$a"
done
shift "$saved"
exec /oath/store/pkg/busybox/bin/tar "$@"
TAR
chmod 755 "$stagedir/steam/libexec/tar"
# check-requirements runs srt-bwrap to test user namespaces. Stub it
# when CLONE_NEWUSER is EPERM. Do not edit steam.sh (updater checksums it).
cat >"$stagedir/steam/libexec/srt-bwrap" <<'BW'
#!/bin/sh
while [ $# -gt 0 ]; do
	case "$1" in
	--bind|--ro-bind|--dev|--tmpfs|--proc|--dir|--chmod|--uid|--gid|--hostname|--chdir|--setenv|--unsetenv)
		shift 2 ;;
	--unshare-user|--unshare-pid|--unshare-net|--unshare-uts|--unshare-ipc|--unshare-cgroup|--unshare-all|--die-with-parent|--as-pid-1|--clearenv|--new-session|--disable-userns)
		shift ;;
	--)
		shift; break ;;
	-*)
		shift ;;
	*)
		break ;;
	esac
done
[ $# -eq 0 ] && exit 0
exec "$@"
BW
chmod 755 "$stagedir/steam/libexec/srt-bwrap"
cat >"$stagedir/steam/libexec/steam-compat.sh" <<'COMPAT'
# sourced by /bin/steam. Host nodes + 32-bit SONAMEs + library path.
# Do not put pkg:sola/lib (64-bit libGL) on LD_LIBRARY_PATH: steamui.so is
# 32-bit and dlmopen errors on ELFCLASS64 instead of skipping.
store=/oath/store/pkg/steam
certs=/oath/store/pkg/sola/etc/ssl/certs/ca-certificates.crt
[ -f "$certs" ] || certs=/oath/store/pkg/curl/ssl/cert.pem
export SSL_CERT_FILE="${SSL_CERT_FILE:-$certs}"
export SSL_CERT_DIR="${SSL_CERT_DIR:-/oath/store/pkg/sola/etc/ssl/certs}"
export CURL_CA_BUNDLE="${CURL_CA_BUNDLE:-$SSL_CERT_FILE}"
export REQUESTS_CA_BUNDLE="${REQUESTS_CA_BUNDLE:-$SSL_CERT_FILE}"
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"
# Seat env points 64-bit mesa at river/dri; 32-bit steamui must not see it.
# 32-bit radeonsi/swrast live in pkg:steam/lib32/dri.
export LIBGL_DRIVERS_PATH=/usr/lib/i386-linux-gnu/dri:/oath/store/pkg/steam/lib32/dri
unset LIBGL_ALWAYS_SOFTWARE
unset __EGL_VENDOR_LIBRARY_FILENAMES
unset GBM_BACKENDS_PATH
sudo -n mkdir -p /usr/bin /lib64 /lib/i386-linux-gnu /sbin /etc/ssl/certs 2>/dev/null || true
sudo -n ln -sfn /bin/env /usr/bin/env 2>/dev/null || true
sudo -n ln -sfn /bin/bash /usr/bin/bash 2>/dev/null || true
sudo -n ln -sfn /oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2 /lib64/ld-linux-x86-64.so.2 2>/dev/null || true
sudo -n ln -sfn libc.so.6 /oath/store/pkg/glibc/lib/libresolv.so.2 2>/dev/null || true
sudo -n ln -sfn /proc/self/fd /dev/fd 2>/dev/null || true
if [ -f "$certs" ]; then
	sudo -n ln -sfn "$certs" /etc/ssl/certs/ca-certificates.crt 2>/dev/null || true
	sudo -n ln -sfn "$certs" /etc/ssl/cert.pem 2>/dev/null || true
fi
if [ -x "$store/libexec/ldconfig" ]; then
	sudo -n ln -sfn "$store/libexec/ldconfig" /sbin/ldconfig 2>/dev/null || true
fi
if [ -x "$store/bin/ldd" ]; then
	sudo -n ln -sfn "$store/bin/ldd" /usr/bin/ldd 2>/dev/null || true
fi
if [ -f /oath/store/pkg/mesa/share/libdrm/amdgpu.ids ]; then
	sudo -n mkdir -p /usr/share/libdrm 2>/dev/null || true
	sudo -n ln -sfn /oath/store/pkg/mesa/share/libdrm/amdgpu.ids /usr/share/libdrm/amdgpu.ids 2>/dev/null || true
fi
if [ -f "$store/lib32/ld-linux.so.2" ]; then
	sudo -n ln -sfn "$store/lib32/ld-linux.so.2" /lib/ld-linux.so.2 2>/dev/null || true
	for f in "$store/lib32"/*.so*; do
		[ -e "$f" ] || continue
		sudo -n ln -sfn "$f" /lib/i386-linux-gnu/"$(basename "$f")" 2>/dev/null || true
	done
fi
# 64-bit srt-logger needs GLib, not mesa. Symlink a GL-free dir.
srtdir=$store/lib/srt
mkdir -p "$srtdir"
for n in libgio-2.0.so.0 libgobject-2.0.so.0 libglib-2.0.so.0 \
	libgmodule-2.0.so.0 libz.so.1 libffi.so.8 libpcre2-8.so.0 \
	libmount.so.1 libselinux.so.1 libblkid.so.1 libcap.so.2 \
	libresolv.so.2 libelf.so.1 liblzma.so.5 libacl.so.1; do
	for src in /oath/store/pkg/sola/lib /oath/store/pkg/river/lib \
		/oath/store/pkg/glibc/lib /oath/store/pkg/gamescope/lib; do
		if [ -e "$src/$n" ]; then
			ln -sfn "$src/$n" "$srtdir/$n"
			break
		fi
	done
done
# steamrt3c ships libGL.so.1.7.0 without a libGL.so.1 SONAME link.
# Point pkg:steam/lib32 at those files so 32-bit dlmopen can find them.
link_sonames() {
	_src=$1
	_want=${2:-1}
	_destdir=$store/lib32
	[ "$_want" = "2" ] && _destdir=$store/lib64
	[ -d "$_src" ] || return 0
	mkdir -p "$_destdir"
	for _f in "$_src"/lib*.so "$_src"/lib*.so.*; do
		[ -f "$_f" ] || continue
		_class=$(od -An -N1 -j4 -tu1 "$_f" 2>/dev/null | tr -d ' \n')
		[ "$_class" = "$_want" ] || continue
		_so=$(patchelf --print-soname "$_f" 2>/dev/null || true)
		[ -n "$_so" ] || _so=$(basename "$_f")
		case "$_so" in
		libc.so.6|libdl.so.2|libm.so.6|libpthread.so.0|librt.so.1|ld-linux.so.2|ld-linux-x86-64.so.2|libresolv.so.2|libstdc++.so.6|libgcc_s.so.1|libdrm.so.2|libdrm_amdgpu.so.1)
			continue ;;
		esac
		# 64-bit CEF must not see steamrt3's old glvnd; use pkg:sola/river.
		if [ "$_want" = "2" ]; then
			case "$_so" in
			libGL.so.1|libEGL.so.1|libGLX.so.0|libGLX_mesa.so.0|libGLdispatch.so.0|libGLESv2.so.2|libgallium-*|libvulkan.so.1)
				continue ;;
			esac
		fi
		ln -sfn "$_f" "$_destdir/$_so"
		ln -sfn "$_f" "$_destdir/$(basename "$_f")"
	done
}
rt="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_32/steam-runtime"
link_sonames "$rt/usr/lib/i386-linux-gnu"
link_sonames "$rt/lib/i386-linux-gnu"
for _d in "${XDG_DATA_HOME:-$HOME/.local/share}/Steam/steamrt64/pv-runtime/steam-runtime-steamrt"/steamrt3c_platform_*/files/lib/i386-linux-gnu \
	"${XDG_DATA_HOME:-$HOME/.local/share}/Steam/steamrt64/pv-runtime/steam-runtime-steamrt"/steamrt3c_platform_*/files/lib/i386-linux-gnu/*/; do
	link_sonames "$_d" 1
done
for _d in "${XDG_DATA_HOME:-$HOME/.local/share}/Steam/steamrt64/pv-runtime/steam-runtime-steamrt"/steamrt3c_platform_*/files/lib/x86_64-linux-gnu \
	"${XDG_DATA_HOME:-$HOME/.local/share}/Steam/steamrt64/pv-runtime/steam-runtime-steamrt"/steamrt3c_platform_*/files/lib/x86_64-linux-gnu/*/; do
	link_sonames "$_d" 2
done
# 32-bit steamui is loaded from $PLATFORM (ubuntu12_32) first. Put GL/gtk
# SONAMEs there so the 32-bit loader never sees 64-bit libGL. This glibc
# errors on wrong ELF class instead of skipping (mixed LD_LIBRARY_PATH dies).
u32="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_32"
if [ -d "$u32" ]; then
	for _f in "$store/lib32"/lib*.so*; do
		[ -e "$_f" ] || continue
		_b=$(basename "$_f")
		case "$_b" in
		libc.so*|libdl.so*|libm.so*|libpthread.so*|librt.so*|ld-linux*|libvulkan.so*) continue ;;
		esac
		[ -e "$u32/$_b" ] && continue
		ln -sfn "$_f" "$u32/$_b"
	done
fi
# Prepend our xz/tar shims so steam.sh extract_archive works.
export PATH="$store/libexec:/bin:/usr/bin"
# 64-bit-only: srt-logger / identify-library-abi. Never mix in lib32.
# pkg:mesa first so 64-bit vulkan/GLX beat steamrt and ubuntu12_32.
export LD_LIBRARY_PATH="/oath/store/pkg/mesa/lib:$srtdir:/oath/store/pkg/glibc/lib"
if [ -f /oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json ]; then
	sudo -n mkdir -p /usr/share/vulkan/icd.d /lib/i386-linux-gnu 2>/dev/null || true
	sudo -n ln -sfn /oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json \
		/usr/share/vulkan/icd.d/radeon_icd.json 2>/dev/null || true
	for f in /oath/store/pkg/mesa/lib32/libvulkan_radeon.so \
		/oath/store/pkg/mesa/lib32/libLLVM.so.21.1 \
		/oath/store/pkg/mesa/lib32/libvulkan.so.1; do
		[ -e "$f" ] || continue
		sudo -n ln -sfn "$f" /lib/i386-linux-gnu/"$(basename "$f")" 2>/dev/null || true
	done
fi
COMPAT
chmod 644 "$stagedir/steam/libexec/steam-compat.sh"
# steamwebhelper.sh execs $STEAM_RUNTIME_STEAMRT/_v2-entry-point.
# Valve checksums ubuntu12_64/steamwebhelper.sh — do not replace it.
# CLONE_NEWUSER is EPERM because PID 1 chrooted from the initrd.
mkdir -p "$stagedir/steam/libexec/pv-host"
cat >"$stagedir/steam/libexec/pv-host/_v2-entry-point" <<'WH'
#!/bin/bash
# Host-side steamwebhelper: skip pressure-vessel. steam.sh documents
# STEAM_RUNTIME_STEAMRT as the unsupported override for this.
set -eu
log() { echo "steamwebhelper-host[$$]: $*" >&2; }
while [ $# -gt 0 ]; do
	case "$1" in
	--) shift; break ;;
	--*) shift ;;
	*) break ;;
	esac
done
if [ $# -lt 1 ]; then
	log "missing steamwebhelper_sniper_wrap.sh"
	exit 1
fi
wrap=$1
shift
dir=$(CDPATH= cd -- "$(dirname "$wrap")" && pwd)
cd "$dir"
store=/oath/store/pkg/steam
export LD_LIBRARY_PATH="$dir:/oath/store/pkg/mesa/lib:${store}/lib64:/oath/store/pkg/river/lib:/oath/store/pkg/xwayland/lib:/oath/store/pkg/glibc/lib"
export LIBGL_DRIVERS_PATH=/oath/store/pkg/mesa/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/mesa/lib/gbm
export __GLX_VENDOR_LIBRARY_NAME=mesa
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json}"
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
export DISABLE_LAYER_MESA_DEVICE_SELECT=1
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export FONTCONFIG_FILE="${FONTCONFIG_FILE:-/oath/store/pkg/sola/etc/fonts/fonts.conf}"
export FONTCONFIG_PATH="${FONTCONFIG_PATH:-/oath/store/pkg/sola/etc/fonts}"
unset LIBGL_ALWAYS_SOFTWARE
case " $* " in
*\ --no-sandbox\ *) ;;
*) set -- --no-sandbox "$@" ;;
esac
log "host (no pressure-vessel) exec ./steamwebhelper $*"
exec ./steamwebhelper "$@"
WH
chmod 755 "$stagedir/steam/libexec/pv-host/_v2-entry-point"
cat >"$stagedir/steam/bin/steam" <<'WRAP'
#!/bin/sh
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
# shellcheck disable=SC1091
. /oath/store/pkg/steam/libexec/steam-compat.sh
# Host River has no XWayland. Rootful pkg:xwayland is the X11 nest
# (gamescope Vulkan still fails on SI/Pitcairn). Never host -f.
if [ -z "${DISPLAY-}" ] && [ -n "${WAYLAND_DISPLAY-}" ]; then
	sudo -n mkdir -p /tmp/.X11-unix /usr/share/X11 /usr/bin 2>/dev/null || true
	sudo -n chmod 1777 /tmp/.X11-unix 2>/dev/null || true
	sudo -n ln -sfn /oath/store/pkg/river/share/X11/xkb /usr/share/X11/xkb 2>/dev/null || true
	if [ -x /oath/store/pkg/xwayland/libexec/xkbcomp ]; then
		sudo -n ln -sfn /oath/store/pkg/xwayland/libexec/xkbcomp /usr/bin/xkbcomp 2>/dev/null || true
	fi
	if [ ! -S /tmp/.X11-unix/X2 ]; then
		/bin/Xwayland :2 -geometry 1920x1080 -decorate -glamor es -noreset -nolisten tcp \
			>/tmp/xwayland.log 2>&1 &
		n=0
		while [ "$n" -lt 30 ]; do
			[ -S /tmp/.X11-unix/X2 ] && break
			sleep 1
			n=$((n + 1))
		done
	fi
	export DISPLAY=:2
fi
if [ -n "${DISPLAY-}" ]; then
	# gamescope sets XDG_CURRENT_DESKTOP=gamescope → Steam forces BPM.
	export XDG_CURRENT_DESKTOP=Sola
	export XDG_SESSION_DESKTOP=Sola
	export XDG_SESSION_TYPE=x11
	unset GAMESCOPE_WAYLAND_DISPLAY
	export SteamDeck=0
	export STEAM_USE_GAMEPADUI=0
	export SteamTenfoot=0
	export SDL_VIDEODRIVER=x11
	export GDK_BACKEND=x11
	export QT_QPA_PLATFORM=xcb
fi
mkdir -p "$HOME/.steam" "$XDG_DATA_HOME/Steam" /tmp/fontconfig
# Valve's launcher is bash. Busybox readlink has no -e.
if grep -q 'readlink -e' /oath/store/pkg/steam/libexec/bin_steam.sh 2>/dev/null; then
	sudo -n sed -i 's/readlink -e -q/readlink -f/g; s/readlink -e/readlink -f/g' \
		/oath/store/pkg/steam/libexec/bin_steam.sh 2>/dev/null || true
fi
rt="$XDG_DATA_HOME/Steam/ubuntu12_32/steam-runtime"
# Bootstrap extract leaves amd64/{lib,usr/lib*} as relative symlinks that
# do not resolve from those directories. check-requirements then looks for
# srt-bwrap under amd64/usr/libexec and dies with ENOENT.
if [ -d "$rt/amd64/usr" ]; then
	ln -sfn ../../usr/libexec "$rt/amd64/usr/libexec" 2>/dev/null || true
	ln -sfn ../../usr/lib "$rt/amd64/usr/lib" 2>/dev/null || true
	ln -sfn ../../usr/share "$rt/amd64/usr/share" 2>/dev/null || true
	ln -sfn ../lib "$rt/amd64/lib" 2>/dev/null || true
fi
# CLONE_NEWUSER is EPERM on this kernel even as root (other nses work).
# Point amd64/usr/libexec at the real tools, then stub srt-bwrap so
# check-requirements exits 0. Do not sed steam.sh (updater checksums it).
if [ -d "$rt/usr/libexec/steam-runtime-tools-0" ] && ! unshare -U true >/dev/null 2>&1; then
	if [ -f "$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap" ] && \
	   [ "$(head -c 4 "$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap" 2>/dev/null)" = $'\x7fELF' ]; then
		mv "$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap" \
			"$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap.real" 2>/dev/null || true
	fi
	cp /oath/store/pkg/steam/libexec/srt-bwrap \
		"$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap" 2>/dev/null || true
	chmod 755 "$rt/usr/libexec/steam-runtime-tools-0/srt-bwrap" 2>/dev/null || true
fi
# steam.sh: STEAM_RUNTIME_STEAMRT overrides steamwebhelper's pressure-vessel
# entry point. Do not replace ubuntu12_64/steamwebhelper.sh (client checksum).
if [ -x /oath/store/pkg/steam/libexec/pv-host/_v2-entry-point ]; then
	export STEAM_RUNTIME_STEAMRT=/oath/store/pkg/steam/libexec/pv-host
fi
exec /bin/bash /oath/store/pkg/steam/libexec/bin_steam.sh "$@"
WRAP
chmod 755 "$stagedir/steam/bin/steam"
cat >"$stagedir/steam/INDEX.md" <<'EOF'
# pkg:steam

Valve steam-launcher (bootstrap tarball + bin_steam.sh) plus a 32-bit
glibc loader for ubuntu12_32/steam. User state is ~/.steam and
~/.local/share/Steam. The /bin/steam wrapper creates /usr/bin/env,
/lib64, CA certs, xz/tar shims, and 32-bit GL SONAMEs beside steamui.so.
steamwebhelper skips pressure-vessel (CLONE_NEWUSER is EPERM after PID 1
chroot) and runs on the host with 64-bit steamrt3 SONAMEs in lib64.
Removable. PID 1 does not supervise Steam.
EOF

echo "==> install sola-arcade"
arcade_elf=${SOLA_ARCADE_ELF:-$root/build/sola-target/release/sola-arcade}
if [ ! -f "$arcade_elf" ]; then
	echo "missing $arcade_elf — run image/build-sola-arcade.sh" >&2
	exit 1
fi
as_root mkdir -p /oath/store/pkg/sola/libexec /oath/store/pkg/sola/bin
as_root cp -a "$arcade_elf" /oath/store/pkg/sola/libexec/sola-arcade
as_root chmod u+w /oath/store/pkg/sola/libexec/sola-arcade
as_root chmod +x /oath/store/pkg/sola/libexec/sola-arcade
rpath="$glibc:$river:/oath/store/pkg/sola/lib:/oath/store/pkg/sola/cef/Release:$pw"
as_root patchelf --set-interpreter "$interp" /oath/store/pkg/sola/libexec/sola-arcade || true
as_root patchelf --set-rpath "$rpath" /oath/store/pkg/sola/libexec/sola-arcade
as_root tee /oath/store/pkg/sola/bin/sola-arcade >/dev/null <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME="${HOME:-/home}"
cd "$HOME" || true
export SHELL="${SHELL:-/bin/thoxa}"
export LANG=C.UTF-8
export LC_ALL=C.UTF-8
export LOCALE_ARCHIVE=/oath/store/pkg/sola/lib/locale/locale-archive
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export XDG_CACHE_HOME=/tmp
export SOLA_NO_SELF_WATCH=1
export SOLA_LOG_DIR=/oath/log
export FONTCONFIG_FILE=/oath/store/pkg/sola/etc/fonts/fonts.conf
export FONTCONFIG_PATH=/oath/store/pkg/sola/etc/fonts
export SOLA_ASSETS_DIR=/oath/store/pkg/sola/share
export SOLA_CEF_DIR=/oath/store/pkg/sola/cef
export XKB_CONFIG_ROOT=/oath/store/pkg/river/share/X11/xkb
export XCURSOR_PATH=/oath/store/pkg/sola/share/cursors
export XCURSOR_THEME=McMojave
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export WGPU_BACKEND=gl
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
export SOLA_OUTPUT_PICK=preferred
/bin/mkdir -p /tmp/fontconfig /oath/log "$HOME/.local/share" "$HOME/.config"
exec /oath/store/pkg/sola/libexec/sola-arcade "$@" >>/oath/log/sola-arcade.log 2>&1
WRAP
as_root chmod 755 /oath/store/pkg/sola/bin/sola-arcade
as_root ln -sfn /oath/store/pkg/sola/bin/sola-arcade /bin/sola-arcade

install_store xwayland "$stagedir/xwayland"
install_store gamescope "$stagedir/gamescope"
install_store mesa "$stagedir/mesa"
install_store steam "$stagedir/steam"
as_root ln -sfn /oath/store/pkg/mesa/bin/vulkaninfo /bin/vulkaninfo

# /bin/steam must not fight an existing name.
for n in Xwayland gamescope steam zenity vulkaninfo; do
	if [ -e /bin/$n ] && [ ! -L /bin/$n ]; then
		as_root rm -f /bin/$n
	fi
done
# glibc 2.42 ships libmvec.so.1 (GLIBC_2.22 vector math). A libmvec→libm
# symlink makes gamescope/vorbis die: version `GLIBC_2.22' not found.
# relocate-pipewire copied the real object into pkg:pipewire; put it back.
if [ -f /oath/store/pkg/pipewire/lib/libmvec.so.1 ]; then
	if [ -L /oath/store/pkg/glibc/lib/libmvec.so.1 ] || [ ! -f /oath/store/pkg/glibc/lib/libmvec.so.1 ]; then
		as_root cp -a /oath/store/pkg/pipewire/lib/libmvec.so.1 /tmp/libmvec.so.1
		as_root mv /tmp/libmvec.so.1 /oath/store/pkg/glibc/lib/libmvec.so.1
		as_root chmod 755 /oath/store/pkg/glibc/lib/libmvec.so.1
	fi
fi
# 32-bit loader at the path the steam ELF encodes.
if [ -f "$store/steam/lib32/ld-linux.so.2" ]; then
	as_root mkdir -p /lib /lib/i386-linux-gnu
	as_root ln -sfn /oath/store/pkg/steam/lib32/ld-linux.so.2 /lib/ld-linux.so.2
	for f in "$store/steam/lib32"/*.so*; do
		[ -e "$f" ] || continue
		as_root ln -sfn "$f" /lib/i386-linux-gnu/"$(basename "$f")"
	done
fi
# Host nodes Steam shebangs / ELF interps / TLS assume. Not the /bin farm.
as_root mkdir -p /usr/bin /lib64 /sbin /etc/ssl/certs
as_root ln -sfn /bin/env /usr/bin/env
as_root ln -sfn /bin/bash /usr/bin/bash
as_root ln -sfn /oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2 /lib64/ld-linux-x86-64.so.2
as_root ln -sfn libc.so.6 /oath/store/pkg/glibc/lib/libresolv.so.2
as_root ln -sfn /proc/self/fd /dev/fd
as_root ln -sfn /oath/store/pkg/steam/libexec/ldconfig /sbin/ldconfig
as_root ln -sfn /oath/store/pkg/steam/bin/ldd /usr/bin/ldd
certs=/oath/store/pkg/sola/etc/ssl/certs/ca-certificates.crt
[ -f "$certs" ] || certs=/oath/store/pkg/curl/ssl/cert.pem
if [ -f "$certs" ]; then
	as_root ln -sfn "$certs" /etc/ssl/certs/ca-certificates.crt
	as_root ln -sfn "$certs" /etc/ssl/cert.pem
fi
# Xwayland looks up /usr/share/X11/xkb/rules/evdev (not XKB_CONFIG_ROOT)
# and xkbcomp on PATH. /tmp/.X11-unix must exist for the display socket.
as_root mkdir -p /usr/share/X11 /tmp/.X11-unix /usr/bin
as_root ln -sfn /oath/store/pkg/river/share/X11/xkb /usr/share/X11/xkb
as_root chmod 1777 /tmp/.X11-unix
if [ -x /oath/store/pkg/xwayland/libexec/xkbcomp ]; then
	as_root ln -sfn /oath/store/pkg/xwayland/libexec/xkbcomp /usr/bin/xkbcomp
	as_root ln -sfn /oath/store/pkg/xwayland/libexec/xkbcomp /bin/xkbcomp
fi

if [ "$(id -u)" = 0 ]; then
	oath apply pkg:xwayland pkg:gamescope pkg:steam
else
	sudo -n oath apply pkg:xwayland pkg:gamescope pkg:steam
fi

echo "==> courage"
for b in bash sola-arcade gamescope Xwayland steam; do
	if [ -x /bin/$b ]; then
		echo "  /bin/$b -> $(readlink /bin/$b 2>/dev/null || echo ELF)"
	else
		echo "  MISSING /bin/$b"
	fi
done
echo 'gamescope needed after rpath:'
patchelf --print-needed /oath/store/pkg/gamescope/libexec/gamescope | head
"$interp" --library-path "$glibc:$store/gamescope/lib:$river:$pw" --list "$store/gamescope/libexec/gamescope" 2>&1 | grep -E 'not found|=>' | head -n 30 || true
echo "done"
