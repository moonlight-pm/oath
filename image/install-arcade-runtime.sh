#!/bin/bash
# Live-install pkg:xwayland, pkg:gamescope, pkg:mesa, pkg:steam and
# sola-arcade on this Oath box. Ubuntu questing debs + Steam bootstrap
# + Debian i386 libc + Debian mesa 26.2.1 GLX.
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

# Ubuntu 2.34+ folded libresolv into libc; this pkg:glibc still ships a
# separate libresolv. tmux NEEDs __b64_pton@GLIBC_2.2.5 from it, and
# tmux's rpath searches pkg:glibc first. Never stub libresolv → libc.
# srt-logger finds it via pkg:steam/lib/srt (pkg:sola copy).
install_real_libresolv() {
	local dest=/oath/store/pkg/glibc/lib/libresolv.so.2
	local src=/oath/store/pkg/sola/lib/libresolv.so.2
	if [ -L "$dest" ]; then
		as_root rm -f "$dest"
	fi
	if [ ! -f "$dest" ] && [ -f "$src" ]; then
		as_root cp -a "$src" "$dest"
		as_root chmod 755 "$dest"
	fi
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
	pool/universe/libd/libdecor-0/libdecor-0-plugin-1-cairo_0.2.2-2_amd64.deb
	pool/main/p/pango1.0/libpango-1.0-0_1.56.3-1build1_amd64.deb
	pool/main/p/pango1.0/libpangocairo-1.0-0_1.56.3-1build1_amd64.deb
	pool/main/p/pango1.0/libpangoft2-1.0-0_1.56.3-1build1_amd64.deb
	pool/main/c/cairo/libcairo2_1.18.4-1build1_amd64.deb
	pool/main/h/harfbuzz/libharfbuzz0b_10.2.0-1_amd64.deb
	pool/main/f/fontconfig/libfontconfig1_2.15.0-2.3ubuntu1_amd64.deb
	pool/main/libt/libthai/libthai0_0.1.29-2build1_amd64.deb
	pool/main/libd/libdatrie/libdatrie1_0.2.13-4_amd64.deb
	pool/main/g/graphite2/libgraphite2-3_1.3.14-2ubuntu1_amd64.deb
	pool/main/f/fribidi/libfribidi0_1.0.16-1_amd64.deb
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
export PATH=/bin:/usr/bin:/oath/store/pkg/gamescope/libexec
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export VK_ICD_FILENAMES=/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
export DISABLE_LAYER_MESA_DEVICE_SELECT=1
export NODEVICE_SELECT=1
# Dual Pitcairn: spare GPU is renderD128. River is on the connected
# card (card1 / renderD129). Nest from the spare imports as black.
_gs_card=
for _c in /sys/class/drm/card[0-9]; do
	_name=$(basename "$_c")
	for _conn in "$_c"/"$_name"-*; do
		[ -f "$_conn/status" ] || continue
		[ "$(cat "$_conn/status" 2>/dev/null)" = connected ] || continue
		_gs_card=$_name
		break
	done
	[ -n "$_gs_card" ] && break
done
if [ -n "$_gs_card" ]; then
	export WLR_DRM_DEVICES=/dev/dri/$_gs_card
	for _r in /sys/class/drm/$_gs_card/device/drm/renderD*; do
		[ -e "$_r" ] || continue
		export OATH_DRM_RENDER=/dev/dri/$(basename "$_r")
		break
	done
fi
# Ubuntu 3.16 pool is short for RADV YCbCr planes (SI vkAllocateDescriptorSets).
export VK_LAYER_PATH=/oath/store/pkg/gamescope/share/vulkan/explicit_layer.d
export VK_INSTANCE_LAYERS=VK_LAYER_OATH_gamescope_pool
export LIBDECOR_PLUGIN_DIR=/oath/store/pkg/gamescope/lib/libdecor/plugins-1
export FONTCONFIG_FILE="${FONTCONFIG_FILE:-/oath/store/pkg/sola/etc/fonts/fonts.conf}"
export FONTCONFIG_PATH="${FONTCONFIG_PATH:-/oath/store/pkg/sola/etc/fonts}"
# Pitcairn has no DRM modifiers. Direct scan-out of client dmabufs to
# River (radeonsi GLES) is GPU tiling garbage; force vulkan composite.
# gamescope --steam also advertises HDR; SI is SDR.
export gamescope_composite_force=true
export gamescope_hdr_enabled=false
export AMD_DEBUG="${AMD_DEBUG:+$AMD_DEBUG,}nodcc"
export RADV_DEBUG="${RADV_DEBUG:+$RADV_DEBUG,}nodcc,nohiz"
export R600_DEBUG="${R600_DEBUG:+$R600_DEBUG,}nodcc"
# libmvec.so.1 is a real glibc object (GLIBC_2.22). Do not let a
# libmvec→libm symlink win; that is "GLIBC_2.22 not found".
unset LD_PRELOAD
export LD_LIBRARY_PATH="/oath/store/pkg/mesa/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/xwayland/lib:/oath/store/pkg/pipewire/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib:/oath/store/pkg/sola/lib"
# wlroots looks up /usr/bin/Xwayland; scripts live under /usr/share/gamescope.
sudo -n mkdir -p /usr/bin /usr/share 2>/dev/null || true
sudo -n ln -sfn /oath/store/pkg/xwayland/bin/Xwayland /usr/bin/Xwayland 2>/dev/null || true
sudo -n ln -sfn /oath/store/pkg/gamescope/share/gamescope /usr/share/gamescope 2>/dev/null || true
# River + libdecor-oath: -b skips CSD and commits xdg 0x0 (segfault).
# Arcade still passes -b (Sola-generic NixOS path). Drop it here.
_gs=
for _a in "$@"; do
	case "$_a" in
	-b|--borderless) continue ;;
	esac
	_gs="${_gs:+$_gs }$_a"
done
# shellcheck disable=SC2086
exec /oath/store/pkg/gamescope/libexec/gamescope $_gs
WRAP
chmod 755 "$stagedir/gamescope/bin/gamescope"

echo "==> pack gamescope descriptor-pool layer"
layer_src=$here/gamescope-pool-layer.c
layer_obj=$stagedir/gamescope-pool-layer.o
layer_so=$stagedir/gamescope/lib/libVkLayer_oath_gamescope_pool.so
if [ ! -f "$layer_src" ]; then
	echo "missing $layer_src" >&2
	exit 1
fi
/bin/cc -c -fPIC -O2 -fno-sanitize=undefined -o "$layer_obj" "$layer_src"
/oath/store/pkg/cc/libexec/zig/zig cc -target x86_64-linux-gnu -shared -O2 -fno-sanitize=undefined \
	-Wl,-rpath,/oath/store/pkg/glibc/lib:/oath/store/pkg/mesa/lib:/oath/store/pkg/river/lib \
	-o "$layer_so" "$layer_obj" -ldl
mkdir -p "$stagedir/gamescope/share/vulkan/explicit_layer.d"
cat >"$stagedir/gamescope/share/vulkan/explicit_layer.d/VkLayer_oath_gamescope_pool.json" <<'JSON'
{
    "file_format_version": "1.2.0",
    "layer": {
        "name": "VK_LAYER_OATH_gamescope_pool",
        "type": "GLOBAL",
        "library_path": "/oath/store/pkg/gamescope/lib/libVkLayer_oath_gamescope_pool.so",
        "api_version": "1.3.0",
        "implementation_version": "1",
        "description": "Pad gamescope descriptor pools for RADV YCbCr planes",
        "disable_environment": {
            "DISABLE_OATH_GAMESCOPE_POOL": "1"
        }
    }
}
JSON

echo "==> pack libdecor plugins (cairo + oath dummy)"
gs_lib=$stagedir/gamescope/lib
gs_rpath="$glibc:$gs_lib:$river:$pw:/oath/store/pkg/sola/lib"
mkdir -p "$gs_lib/libdecor/plugins-1"
deb_lib=$stagedir/debroot/usr/lib/x86_64-linux-gnu
cairo_plug=
if [ -f /tmp/gs/usr/lib/x86_64-linux-gnu/libdecor/plugins-1/libdecor-cairo.so ]; then
	cairo_plug=/tmp/gs/usr/lib/x86_64-linux-gnu/libdecor/plugins-1/libdecor-cairo.so
elif [ -f "$deb_lib/libdecor/plugins-1/libdecor-cairo.so" ]; then
	cairo_plug=$deb_lib/libdecor/plugins-1/libdecor-cairo.so
fi
if [ -n "$cairo_plug" ]; then
	cp -a "$cairo_plug" "$gs_lib/libdecor/plugins-1/libdecor-cairo.so"
	chmod u+w "$gs_lib/libdecor/plugins-1/libdecor-cairo.so"
fi
copy_gs_so() {
	local pat=$1
	local f
	f=$(find "$deb_lib" -name "$pat" ! -type l | head -1)
	[ -n "$f" ] || return 0
	cp -a "$f" "$gs_lib/$(basename "$f")"
	chmod u+w "$gs_lib/$(basename "$f")"
	local so
	so=$(patchelf --print-soname "$gs_lib/$(basename "$f")" 2>/dev/null || true)
	if [ -n "$so" ] && [ "$so" != "$(basename "$f")" ]; then
		ln -sfn "$(basename "$f")" "$gs_lib/$so"
	fi
}
for pat in \
	'libpangocairo-1.0.so.0*' \
	'libpangoft2-1.0.so.0*' \
	'libpango-1.0.so.0*' \
	'libcairo.so.2*' \
	'libharfbuzz.so.0*' \
	'libfontconfig.so.1*' \
	'libthai.so.0*' \
	'libdatrie.so.1*' \
	'libgraphite2.so.3*' \
	'libfribidi.so.0*'
do
	copy_gs_so "$pat"
done
find "$gs_lib" "$gs_lib/libdecor/plugins-1" -maxdepth 1 -type f \( \
	-name 'libpango*' -o -name 'libcairo.so.2*' -o -name 'libharfbuzz*' \
	-o -name 'libfontconfig.so.1*' -o -name 'libthai*' -o -name 'libdatrie*' \
	-o -name 'libgraphite2*' -o -name 'libfribidi*' -o -name 'libdecor-cairo.so' \
\) | while read -r f; do
	is_elf "$f" || continue
	chmod u+w "$f" || true
	patchelf --set-rpath "$gs_rpath" "$f" 2>/dev/null || true
done
# Dummy wins (HIGH). Cairo mmap-crashes on 0-size CSD; dummy reports 1px
# borders so first xdg geometry is 2x2 instead of 0x0.
oath_plug_src=$here/libdecor-oath-plugin.c
oath_plug_obj=$stagedir/libdecor-oath-plugin.o
oath_plug_so=$gs_lib/libdecor/plugins-1/libdecor-oath.so
if [ ! -f "$oath_plug_src" ]; then
	echo "missing $oath_plug_src" >&2
	exit 1
fi
decor_so=
for f in "$gs_lib"/libdecor-0.so.0*; do
	[ -f "$f" ] && [ ! -L "$f" ] && decor_so=$f
done
[ -n "$decor_so" ] || { echo "missing libdecor-0.so in gamescope pack" >&2; exit 1; }
/bin/cc -c -fPIC -O2 -fno-sanitize=undefined -o "$oath_plug_obj" "$oath_plug_src"
/oath/store/pkg/cc/libexec/zig/zig cc -target x86_64-linux-gnu -shared -O2 -fno-sanitize=undefined \
	-Wl,-rpath,/oath/store/pkg/gamescope/lib:/oath/store/pkg/glibc/lib \
	-o "$oath_plug_so" "$oath_plug_obj" "$decor_so"

cat >"$stagedir/gamescope/INDEX.md" <<'EOF'
# pkg:gamescope

Windowed nest compositor for sola-arcade. Ubuntu questing 3.16 gamescope
relocated onto pkg:glibc + pkg:river. Removable. PID 1 does not supervise it.
VK_LAYER_OATH_gamescope_pool pads descriptor pools so RADV SI can
vkAllocateDescriptorSets (YCbCr planes). libdecor-cairo is packed;
libdecor-oath (HIGH) reports 1px borders so River accepts the first
xdg geometry (cairo mmap-crashes on a 0-size CSD buffer).
EOF

echo "==> pack xwayland"
extract_deb "$fetchdir/xwayland_24.1.13-1_amd64.deb" "$stagedir/debroot"
relocate_bins "$stagedir/xwayland" "$stagedir/debroot/usr/bin/Xwayland"
cat >"$stagedir/xwayland/bin/Xwayland" <<'WRAP'
#!/bin/sh
export PATH=/bin:/usr/bin
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
# Nested glamor on river radeonsi SIGBUS'd on SI (GEM map of tiled
# BOs). mesa 26.2.1 libgallium needs GLIBC_2.43. -glamor off keeps
# DRI3 (Vulkan WSI); XWAYLAND_NO_GLAMOR would drop DRI3 too.
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export XKB_CONFIG_ROOT="${XKB_CONFIG_ROOT:-/oath/store/pkg/river/share/X11/xkb}"
export XKB_BINDIR=/oath/store/pkg/xwayland/libexec
unset LD_PRELOAD
export LD_LIBRARY_PATH="/oath/store/pkg/xwayland/lib:/oath/store/pkg/gamescope/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib:/oath/store/pkg/sola/lib"
exec /oath/store/pkg/xwayland/libexec/Xwayland -glamor off "$@"
WRAP
chmod 755 "$stagedir/xwayland/bin/Xwayland"
# gamescope looks up Xwayland on PATH
ln -sfn /oath/store/pkg/xwayland/bin/Xwayland "$stagedir/gamescope/bin/Xwayland" 2>/dev/null || true

echo "==> pack xwayland clipboard bridge"
# Rootful Xwayland does not share CLIPBOARD with the compositor. wl-paste
# watches wlr-data-control; xclip owns X11 CLIPBOARD so Ctrl+V pastes.
clip_mirror=https://deb.debian.org/debian/pool/main
for rel in x/xclip/xclip_0.13-4_amd64.deb w/wl-clipboard/wl-clipboard_2.2.1-2_amd64.deb; do
	dest=$fetchdir/$(basename "$rel")
	if [ ! -f "$dest" ] || [ ! -s "$dest" ]; then
		echo "fetch $clip_mirror/$rel"
		curl -fL --retry 3 --retry-delay 2 -o "$dest" "$clip_mirror/$rel"
	fi
	extract_deb "$dest" "$stagedir/debroot"
done
clip_rpath="$glibc:/oath/store/pkg/xwayland/lib:$river"
for b in xclip wl-paste wl-copy; do
	src=$stagedir/debroot/usr/bin/$b
	[ -f "$src" ] || { echo "missing $src" >&2; exit 1; }
	cp -a "$src" "$stagedir/xwayland/libexec/$b"
	chmod u+w "$stagedir/xwayland/libexec/$b"
	patchelf --set-interpreter "$interp" "$stagedir/xwayland/libexec/$b"
	patchelf --set-rpath "$clip_rpath" "$stagedir/xwayland/libexec/$b"
done
# libXmu → libXt → libSM → libuuid (not always in the Xwayland NEEDED set).
uuid=
for f in "$stagedir/gamescope/lib"/libuuid.so.1.* \
	"$stagedir/debroot"/usr/lib/x86_64-linux-gnu/libuuid.so.1.* \
	"$stagedir/xwayland/lib"/libuuid.so.1.*; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	uuid=$f
	break
done
if [ -n "$uuid" ]; then
	cp -a "$uuid" "$stagedir/xwayland/lib/$(basename "$uuid")"
	ln -sfn "$(basename "$uuid")" "$stagedir/xwayland/lib/libuuid.so.1"
fi
install -m 755 "$here/xwayland-clip.sh" "$stagedir/xwayland/libexec/xwayland-clip"
install -m 755 "$here/xwayland-clip-in.sh" "$stagedir/xwayland/libexec/xwayland-clip-in"

echo "==> pack oath-xwm"
zig_cc=${zig_cc:-/oath/store/pkg/cc/libexec/zig/zig}
xcb_so=
for f in "$stagedir/xwayland/lib"/libxcb.so.1*; do
	[ -f "$f" ] && [ ! -L "$f" ] && xcb_so=$f && break
done
[ -n "$xcb_so" ] || xcb_so=$stagedir/xwayland/lib/libxcb.so.1
if [ -x "$zig_cc" ] && [ -f "$here/oath-xwm.c" ] && [ -e "$xcb_so" ]; then
	"$zig_cc" cc -target x86_64-linux-gnu -O2 -fno-sanitize=undefined \
		-Wl,-rpath,/oath/store/pkg/xwayland/lib:/oath/store/pkg/glibc/lib \
		-o "$stagedir/xwayland/libexec/oath-xwm" \
		"$here/oath-xwm.c" "$xcb_so" || \
		echo "warn: oath-xwm not built" >&2
	if [ -x "$stagedir/xwayland/libexec/oath-xwm" ]; then
		patchelf --set-interpreter "$interp" "$stagedir/xwayland/libexec/oath-xwm" || true
		chmod 755 "$stagedir/xwayland/libexec/oath-xwm"
	fi
fi

cat >"$stagedir/xwayland/INDEX.md" <<'EOF'
# pkg:xwayland

Xwayland for gamescope's nested X and (later) host River. Debian 24.1
relocated onto pkg:glibc + pkg:river. Nested Xwayland is `-glamor off`
(river radeonsi SIGBUS'd on SI tiled BOs; mesa 26.2.1 libgallium
needs GLIBC_2.43). Removable.

Rootful `Xwayland :2 -decorate` (Steam nest) does not share CLIPBOARD
with Wayland. `libexec/xwayland-clip` watches the compositor clipboard
(`wl-paste`) and owns X11 CLIPBOARD (`xclip`) so Ctrl+V pastes.

Rootful Xwayland has no WM. Steam's library window then lands at
INT_MIN and the nest is black. `libexec/oath-xwm` maps and clamps
windows onto the screen and holds one InputOnly client so the
Wayland surface survives login → library.
EOF

echo "==> pack mesa (64-bit GLX)"
debian_mirror=https://deb.debian.org/debian/pool/main
fetch_debian() {
	local rel=$1
	local dest=$fetchdir/$(basename "$rel")
	local url=$debian_mirror/$(printf '%s' "$rel" | sed 's/+/%2B/g')
	if [ -f "$dest" ] && [ -s "$dest" ]; then
		echo "cached $dest"
		return 0
	fi
	echo "fetch $url"
	curl -fL --retry 3 --retry-delay 2 -o "$dest" "$url"
}
for rel in \
	m/mesa/libglx-mesa0_26.2.1-4_amd64.deb \
	m/mesa/mesa-libgallium_26.2.1-4_amd64.deb \
	m/mesa/libgl1-mesa-dri_26.2.1-4_amd64.deb \
	m/mesa/libgbm1_26.2.1-4_amd64.deb \
	m/mesa/mesa-vulkan-drivers_26.2.1-4_amd64.deb \
	libg/libglvnd/libgl1_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglx0_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libglvnd0_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libegl1_1.7.0-3+b1_amd64.deb \
	libg/libglvnd/libgles2_1.7.0-3+b1_amd64.deb \
	m/mesa/libegl-mesa0_26.2.1-4_amd64.deb \
	libx/libxcb/libxcb-glx0_1.17.0-2+b2_amd64.deb \
	libd/libdrm/libdrm-common_2.4.124-2_all.deb \
	v/vulkan-loader/libvulkan1_1.4.357.0-1_amd64.deb \
	v/vulkan-tools/vulkan-tools_1.4.341.0+dfsg1-1_amd64.deb
do
	fetch_debian "$rel"
	extract_deb "$fetchdir/$(basename "$rel")" "$stagedir/debroot"
done
rm -rf "$stagedir/mesa"
mkdir -p "$stagedir/mesa/lib/dri" "$stagedir/mesa/lib/gbm" "$stagedir/mesa/share/libdrm" \
	"$stagedir/mesa/share/glvnd/egl_vendor.d"
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
copy_mesa "$mesa_src/libEGL.so.1.1.0" "$stagedir/mesa/lib/libEGL.so.1.1.0"
copy_mesa "$mesa_src/libEGL_mesa.so.0.0.0" "$stagedir/mesa/lib/libEGL_mesa.so.0.0.0"
copy_mesa "$mesa_src/libGLESv2.so.2.1.0" "$stagedir/mesa/lib/libGLESv2.so.2.1.0"
gallium_so=$(find "$mesa_src" -maxdepth 1 -name 'libgallium-*.so' ! -type l | head -1)
[ -n "$gallium_so" ] || { echo "missing libgallium in $mesa_src" >&2; exit 1; }
copy_mesa "$gallium_so" "$stagedir/mesa/lib/$(basename "$gallium_so")"
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
ln -sfn libEGL.so.1.1.0 "$stagedir/mesa/lib/libEGL.so.1"
ln -sfn libEGL_mesa.so.0.0.0 "$stagedir/mesa/lib/libEGL_mesa.so.0"
ln -sfn libGLESv2.so.2.1.0 "$stagedir/mesa/lib/libGLESv2.so.2"
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
cat >"$stagedir/mesa/share/glvnd/egl_vendor.d/50_mesa.json" <<'JSON'
{
    "file_format_version" : "1.0.0",
    "ICD" : {
        "library_path" : "/oath/store/pkg/mesa/lib/libEGL_mesa.so.0"
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
fetch_debian m/mesa/mesa-vulkan-drivers_26.2.1-4_i386.deb
fetch_debian l/llvm-toolchain-21/libllvm21_21.1.8-10_i386.deb
fetch_debian v/vulkan-loader/libvulkan1_1.4.357.0-1_i386.deb
# RADV NEEDs libdisplay-info.so.3; LLVM 21 NEEDs libxml2.so.16.
# steam/lib32 already has drm/xcb/ffi/z3/edit/atomic.
# steamrt's libwayland-client 0.3.0 has no wl_fixes_interface; Mesa 26
# RADV fails to dlopen (loader then reports no VK_KHR_surface). Pack
# Debian 1.26 beside the ICD so rpath finds it first.
fetch_debian libd/libdisplay-info/libdisplay-info3_0.3.0-1+b1_i386.deb
fetch_debian libx/libxml2/libxml2-16_2.15.3+dfsg-1_i386.deb
fetch_debian w/wayland/libwayland-client0_1.26.0-1_i386.deb
fetch_debian m/mesa/libgbm1_26.2.1-4_i386.deb
extract_deb "$fetchdir/mesa-vulkan-drivers_26.2.1-4_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libllvm21_21.1.8-10_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libvulkan1_1.4.357.0-1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libdisplay-info3_0.3.0-1+b1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libxml2-16_2.15.3+dfsg-1_i386.deb" "$stagedir/debroot32"
extract_deb "$fetchdir/libwayland-client0_1.26.0-1_i386.deb" "$stagedir/debroot32"
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
ln -sfn "$(basename "$di32")" "$stagedir/mesa/lib32/libdisplay-info.so.3"
ln -sfn "$(basename "$xml32")" "$stagedir/mesa/lib32/libxml2.so.16"
ln -sfn "$(basename "$wl32")" "$stagedir/mesa/lib32/libwayland-client.so.0"
gbm32=$(find "$stagedir/debroot32" -name 'libgbm.so.1.0.0' ! -type l | head -1)
if [ -n "$gbm32" ]; then
	copy_mesa "$gbm32" "$stagedir/mesa/lib32/libgbm.so.1.0.0"
	ln -sfn libgbm.so.1.0.0 "$stagedir/mesa/lib32/libgbm.so.1"
fi
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
# Steam sets LD_LIBRARY_PATH to steamrt (old wayland). DT_RUNPATH loses;
# DT_RPATH on the ICD wins so wl_fixes resolves.
patchelf --force-rpath --set-rpath "$rpath32" \
	"$stagedir/mesa/lib32/libvulkan_radeon.so"
patchelf --force-rpath --set-rpath "$mesa_rpath" \
	"$stagedir/mesa/lib/libvulkan_radeon.so"
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

64-bit OpenGL/GLX/EGL and Vulkan WSI for X11/Wayland clients. Debian mesa
26.2.1 GLX + EGL + glvnd + gallium + RADV, plus Khronos vulkan-loader
1.4.357 and vulkaninfo. Nested Xwayland is `-glamor off` (this libgallium needs GLIBC_2.43;
pkg:glibc is 2.42). DRI is libdril → radeonsi. ICD is
share/vulkan/icd.d/radeon_icd.json (64-bit) and radeon_icd32.json
(32-bit Steam). lib32 ships RADV + LLVM 21 + libdisplay-info.so.3 +
libxml2.so.16 + libwayland-client 1.26 (`wl_fixes_interface`; steamrt
0.3.0 is too old). ICD uses DT_RPATH so Steam's steamrt
LD_LIBRARY_PATH cannot hide the new wayland. 64-bit LLVM stays in
pkg:river. Removable.
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
# steam.sh: STEAM_DEBUGGER=${DEBUGGER-} runs after extract, before the
# ubuntu12_32/steam ELF. Re-apply dlmopen shims the updater clobbers.
cat >"$stagedir/steam/libexec/oath-steam-preexec" <<'PRE'
#!/bin/sh
ui="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_32/steamui.so"
u32="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_32"
so=/oath/store/pkg/steam/lib32/liboath-peercred.so
if [ -f "$so" ] && [ -d "$u32" ]; then
	ln -sfn "$so" "$u32/liboath-peercred.so"
fi
# Do not patchelf live steamui.so — Steam verifies size and re-extracts.
# dlmopen in liboath-peercred.so loads a patched copy from /tmp.
echo "oath-steam-preexec: shim=$so ui=$ui" >&2
exec "$@"
PRE
chmod 755 "$stagedir/steam/libexec/oath-steam-preexec"
# Deck UI shells out to SteamOS helpers. Stub the ones that abort
# GetSystemVersionDetails; do not invent a second OS.
mkdir -p "$stagedir/steam/libexec/steamos-polkit-helpers"
cat >"$stagedir/steam/libexec/steamos-select-branch" <<'STUB'
#!/bin/sh
echo rel
STUB
cat >"$stagedir/steam/libexec/lsb_release" <<'STUB'
#!/bin/sh
case "$1" in
-is|--id) echo SteamOS ;;
-rs|--release) echo 3.7 ;;
-ds|--description) echo "SteamOS 3.7" ;;
*) echo "SteamOS 3.7" ;;
esac
STUB
cat >"$stagedir/steam/libexec/timedatectl" <<'STUB'
#!/bin/sh
exit 0
STUB
cat >"$stagedir/steam/libexec/steamos-polkit-helpers/steamos-devkit-mode" <<'STUB'
#!/bin/sh
exit 0
STUB
cat >"$stagedir/steam/libexec/steamos-polkit-helpers/jupiter-dock-updater" <<'STUB'
#!/bin/sh
exit 0
STUB
chmod 755 "$stagedir/steam/libexec/steamos-select-branch" \
	"$stagedir/steam/libexec/lsb_release" \
	"$stagedir/steam/libexec/timedatectl" \
	"$stagedir/steam/libexec/steamos-polkit-helpers/steamos-devkit-mode" \
	"$stagedir/steam/libexec/steamos-polkit-helpers/jupiter-dock-updater"
# 32-bit preload: steamui SO_PEERCRED on TCP returns pid 0.
zig_cc=/oath/store/pkg/cc/libexec/zig/zig
if [ -x "$zig_cc" ] && [ -f "$here/oath-steam-peercred.c" ]; then
	"$zig_cc" cc -target x86-linux-gnu -shared -fPIC -O2 \
		-L "$stagedir/steam/lib32" \
		-Wl,-rpath,/oath/store/pkg/steam/lib32 \
		-o "$stagedir/steam/lib32/liboath-peercred.so" \
		"$here/oath-steam-peercred.c" || \
		echo "warn: liboath-peercred.so not built" >&2
fi
mkdir -p "$stagedir/steam/lib64"
if [ -x "$zig_cc" ] && [ -f "$here/oath-steam-dumpable.c" ]; then
	"$zig_cc" cc -target x86_64-linux-gnu -shared -fPIC -O2 \
		-Wl,-rpath,/oath/store/pkg/glibc/lib \
		-o "$stagedir/steam/lib64/liboath-dumpable.so" \
		"$here/oath-steam-dumpable.c" || \
		echo "warn: liboath-dumpable.so not built" >&2
fi
if [ -x "$zig_cc" ] && [ -f "$here/oath-glclass.c" ]; then
	"$zig_cc" cc -target x86_64-linux-gnu -shared -fPIC -O2 \
		-Wl,-rpath,/oath/store/pkg/glibc/lib \
		-o "$stagedir/steam/lib64/liboath-glclass.so" \
		"$here/oath-glclass.c" || \
		echo "warn: liboath-glclass.so not built" >&2
fi
if [ -x "$zig_cc" ] && [ -f "$here/oath-cefgeom.c" ]; then
	"$zig_cc" cc -target x86_64-linux-gnu -shared -fPIC -O2 \
		-Wl,-rpath,/oath/store/pkg/glibc/lib \
		-o "$stagedir/steam/lib64/liboath-cefgeom.so" \
		"$here/oath-cefgeom.c" -ldl || \
		echo "warn: liboath-cefgeom.so not built" >&2
fi
if [ -x "$zig_cc" ] && [ -f "$here/oath-lsof.c" ]; then
	"$zig_cc" cc -target x86_64-linux-musl -static -O2 \
		-o "$stagedir/steam/libexec/oath-lsof" \
		"$here/oath-lsof.c" || \
		echo "warn: oath-lsof not built" >&2
	if [ -x "$stagedir/steam/libexec/oath-lsof" ]; then
		chmod 755 "$stagedir/steam/libexec/oath-lsof"
	fi
fi
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
sudo -n mkdir -p /usr/bin /lib64 /lib/i386-linux-gnu /sbin /etc/ssl/certs /usr/share/X11 2>/dev/null || true
sudo -n ln -sfn /bin/env /usr/bin/env 2>/dev/null || true
sudo -n ln -sfn /bin/bash /usr/bin/bash 2>/dev/null || true
# libX11 i18n: compiled-in XLOCALEDIR is /usr/share/X11/locale. Missing
# locale.dir makes _XlcCreateLocaleDataBase NULL-deref in steamui.
# compose.dir has no C.UTF-8 row (only `Compose C` → iso8859-1), so
# XOpenIM() fails even after locale.dir exists.
rt="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_32/steam-runtime"
if [ -d "$rt/usr/share/X11/locale" ]; then
	sudo -n ln -sfn "$rt/usr/share/X11/locale" /usr/share/X11/locale 2>/dev/null || true
	sudo -n mkdir -p /usr/lib/i386-linux-gnu/X11 2>/dev/null || true
	sudo -n ln -sfn /usr/share/X11/locale /usr/lib/i386-linux-gnu/X11/locale 2>/dev/null || true
	export XLOCALEDIR=/usr/share/X11/locale
	# Do not grep 'C.UTF-8' as a regex: it matches es_EC.UTF-8.
	if [ -f "$rt/usr/share/X11/locale/compose.dir" ] &&
	    ! awk '$NF=="C.UTF-8"{f=1} END{exit !f}' "$rt/usr/share/X11/locale/compose.dir"; then
		printf '%s\n' 'en_US.UTF-8/Compose	C.UTF-8' \
			>> "$rt/usr/share/X11/locale/compose.dir" || true
	fi
	if [ -f "$rt/usr/share/X11/locale/locale.dir" ] &&
	    ! awk '$NF=="C.UTF-8"{f=1} END{exit !f}' "$rt/usr/share/X11/locale/locale.dir"; then
		printf '%s\n' 'en_US.UTF-8/XLC_LOCALE	C.UTF-8' \
			>> "$rt/usr/share/X11/locale/locale.dir" || true
	fi
fi
# Nested gamescope --steam advertises HDR and then scan-outs client
# dmabufs. Pitcairn has no DRM modifiers: that pass-through is static
# on River (radeonsi). Force SDR + re-assert composite after Steam's
# CGamescopeController sets composite_force 0.
if [ -n "${GAMESCOPE_WAYLAND_DISPLAY-}" ]; then
	export STEAM_GAMESCOPE_HDR_SUPPORTED=0
	export ENABLE_HDR_WSI=0
	export DXVK_HDR=0
	(
		ctl=/oath/store/pkg/gamescope/libexec/gamescopectl
		[ -x "$ctl" ] || exit 0
		export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
		unset LD_PRELOAD
		i=0
		while [ "$i" -lt 8 ]; do
			sleep 12
			"$ctl" hdr_enabled 0
			"$ctl" composite_force 1
			i=$((i + 1))
		done
	) >/tmp/oath-gamescope-convar.log 2>&1 &
fi
sudo -n ln -sfn /oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2 /lib64/ld-linux-x86-64.so.2 2>/dev/null || true
# Do not stub pkg:glibc libresolv → libc (tmux __b64_pton; rpath glibc first).
if [ -L /oath/store/pkg/glibc/lib/libresolv.so.2 ]; then
	sudo -n rm -f /oath/store/pkg/glibc/lib/libresolv.so.2 2>/dev/null || true
	if [ -f /oath/store/pkg/sola/lib/libresolv.so.2 ]; then
		sudo -n cp -a /oath/store/pkg/sola/lib/libresolv.so.2 \
			/oath/store/pkg/glibc/lib/libresolv.so.2 2>/dev/null || true
		sudo -n chmod 755 /oath/store/pkg/glibc/lib/libresolv.so.2 2>/dev/null || true
	fi
fi
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
# 64-bit srt-logger needs GLib from $srtdir. Do not put 64-bit mesa/lib
# on this path: 32-bit steam then dlopens ELFCLASS64 libvulkan and the
# ICD never loads. 64-bit vulkan is /lib64/libvulkan.so.1 (below).
export LD_LIBRARY_PATH="$srtdir:/oath/store/pkg/glibc/lib"
if [ -f /oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json ]; then
	sudo -n mkdir -p /usr/share/vulkan/icd.d /lib/i386-linux-gnu /lib64 2>/dev/null || true
	# Both ICDs: the loader skips the wrong ELF class. A single
	# radeon_icd.json pointing at lib32 makes 64-bit steamsysinfo
	# vkCreateInstance return VK_ERROR_INCOMPATIBLE_DRIVER (-9).
	sudo -n ln -sfn /oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json \
		/usr/share/vulkan/icd.d/radeon_icd.x86_64.json 2>/dev/null || true
	sudo -n ln -sfn /oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json \
		/usr/share/vulkan/icd.d/radeon_icd.i686.json 2>/dev/null || true
	sudo -n rm -f /usr/share/vulkan/icd.d/radeon_icd.json 2>/dev/null || true
	sudo -n ln -sfn /oath/store/pkg/mesa/lib/libvulkan.so.1 \
		/lib64/libvulkan.so.1 2>/dev/null || true
	for n in libGL.so.1 libGLX.so.0 libGLdispatch.so.0 libEGL.so.1 \
		libGLESv2.so.2 libgbm.so.1 libGLX_mesa.so.0 libEGL_mesa.so.0; do
		if [ -e /oath/store/pkg/mesa/lib/$n ]; then
			sudo -n ln -sfn /oath/store/pkg/mesa/lib/$n /lib64/$n 2>/dev/null || true
		fi
	done
	for f in /oath/store/pkg/mesa/lib32/lib*.so*; do
		[ -e "$f" ] || continue
		sudo -n ln -sfn "$f" /lib/i386-linux-gnu/"$(basename "$f")" 2>/dev/null || true
	done
fi
u64="${XDG_DATA_HOME:-$HOME/.local/share}/Steam/ubuntu12_64"
if [ -d "$u64" ]; then
	for n in libGL.so.1 libEGL.so.1 libGLX.so.0 libGLdispatch.so.0 libGLESv2.so.2; do
		if [ -e /oath/store/pkg/mesa/lib/$n ]; then
			ln -sfn /oath/store/pkg/mesa/lib/$n "$u64/$n"
		fi
	done
fi
export VK_ICD_FILENAMES="/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json:/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json"
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
_preload=""
if [ -f /oath/store/pkg/steam/lib32/liboath-peercred.so ]; then
	# 32-bit only; 64-bit helpers ignore the wrong ELF class.
	_preload=/oath/store/pkg/steam/lib32/liboath-peercred.so
fi
if [ -f /oath/store/pkg/steam/lib64/liboath-glclass.so ]; then
	# 64-bit only; 32-bit steam ignores ELFCLASS64. Stops gldriverquery
	# loading ubuntu12_32's 32-bit libGL.so.1.
	_preload="${_preload:+$_preload:}/oath/store/pkg/steam/lib64/liboath-glclass.so"
fi
if [ -n "$_preload" ]; then
	export LD_PRELOAD="${_preload}${LD_PRELOAD:+:$LD_PRELOAD}"
fi
# Do not patchelf live steamui.so (verifier re-extracts). oath-steam-preexec
# dlmopen's a copy.
# glibc nsswitch on the first images omitted hosts:. Steam CEF looks up
# steamloopback.host; PID 1 rewrites /etc/hosts without that alias.
if ! grep -q '^hosts:' /etc/nsswitch.conf 2>/dev/null; then
	sudo -n sh -c 'printf "%s\n" "hosts: files dns" >> /etc/nsswitch.conf' 2>/dev/null || true
fi
if ! grep -q 'steamloopback.host' /etc/hosts 2>/dev/null; then
	sudo -n sh -c 'printf "%s\n" "127.0.0.1 steamloopback.host" >> /etc/hosts' 2>/dev/null || true
fi
if [ -x /oath/store/pkg/steam/libexec/oath-lsof ]; then
	sudo -n mkdir -p /usr/bin /usr/sbin /sbin 2>/dev/null || true
	if [ ! -x /usr/bin/lsof ] || ! grep -q oath-lsof /usr/bin/lsof 2>/dev/null; then
		sudo -n sh -c 'printf "%s\n" "#!/bin/sh" "exec sudo -n /oath/store/pkg/steam/libexec/oath-lsof \"\$@\"" > /usr/bin/lsof' 2>/dev/null || true
		sudo -n chmod 755 /usr/bin/lsof 2>/dev/null || true
	fi
	sudo -n ln -sfn /usr/bin/lsof /bin/lsof 2>/dev/null || true
	sudo -n ln -sfn /usr/bin/lsof /sbin/lsof 2>/dev/null || true
	sudo -n ln -sfn /usr/bin/lsof /usr/sbin/lsof 2>/dev/null || true
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
export DISABLE_LAYER_MESA_DEVICE_SELECT=1
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export FONTCONFIG_FILE="${FONTCONFIG_FILE:-/oath/store/pkg/sola/etc/fonts/fonts.conf}"
export FONTCONFIG_PATH="${FONTCONFIG_PATH:-/oath/store/pkg/sola/etc/fonts}"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd.json:/oath/store/pkg/mesa/share/vulkan/icd.d/radeon_icd32.json}"
export VK_DRIVER_FILES="$VK_ICD_FILENAMES"
unset LIBGL_ALWAYS_SOFTWARE
unset LD_PRELOAD
# Pitcairn RADV SI: CEF GPU process SIGBUS (exit 135) compositing the
# 0x0 library browser. Software compositing is enough for Deck chrome.
export RADV_DEBUG="${RADV_DEBUG:-nodcc,nohiz}"
_pre=
if [ -f /oath/store/pkg/steam/lib64/liboath-dumpable.so ]; then
	_pre=/oath/store/pkg/steam/lib64/liboath-dumpable.so
fi
if [ -f /oath/store/pkg/steam/lib64/liboath-cefgeom.so ]; then
	_pre="${_pre:+$_pre:}/oath/store/pkg/steam/lib64/liboath-cefgeom.so"
fi
if [ -n "$_pre" ]; then
	export LD_PRELOAD="$_pre"
fi
case " $* " in
*\ --no-sandbox\ *) ;;
*) set -- --no-sandbox "$@" ;;
esac
case " $* " in
*\ --disable-gpu\ *) ;;
*) set -- --disable-gpu --disable-gpu-compositing "$@" ;;
esac
# steamui WebUITransport matches the websocket inode in
# /proc/<webhelper-pid>/fd. CEF's network utility process owns the TCP
# socket otherwise (Checked: 0/<pid> → reject → segfault).
feats_done=0
args=()
for a in "$@"; do
	case "$a" in
	--enable-features=*)
		case "$a" in
		*NetworkServiceInProcess*) ;;
		*) a="$a,NetworkServiceInProcess" ;;
		esac
		feats_done=1
		;;
	esac
	args+=("$a")
done
set -- "${args[@]}"
if [ "$feats_done" = 0 ]; then
	set -- --enable-features=NetworkServiceInProcess "$@"
fi
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
if [ -z "${WAYLAND_DISPLAY-}" ] && [ -S "${XDG_RUNTIME_DIR:-/run/user/1}/wayland-1" ]; then
	export WAYLAND_DISPLAY=wayland-1
fi
# Nest is gamescope as a Wayland client (T37). Never host -f. Rootful
# Xwayland :2 was a workaround while gamescope Vulkan was still
# failing on SI; the nest window is in (RADV PITCAIRN + libdecor-oath).
if [ -z "${GAMESCOPE_WAYLAND_DISPLAY-}" ] && [ -n "${WAYLAND_DISPLAY-}" ] && [ -x /bin/gamescope ]; then
	nest=0
	case "${DISPLAY-}" in
	""|:2|:2.*) nest=1 ;;
	esac
	if [ "$nest" = 1 ]; then
		sudo -n mkdir -p /tmp/.X11-unix /usr/share/X11 /usr/bin 2>/dev/null || true
		sudo -n chmod 1777 /tmp/.X11-unix 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/river/share/X11/xkb /usr/share/X11/xkb 2>/dev/null || true
		if [ -x /oath/store/pkg/xwayland/libexec/xkbcomp ]; then
			sudo -n ln -sfn /oath/store/pkg/xwayland/libexec/xkbcomp /usr/bin/xkbcomp 2>/dev/null || true
		fi
		unset DISPLAY
		# Do not pass -b (borderless): that skips libdecor and
		# commits xdg geometry 0x0, which segfaults gamescope on
		# this River. libdecor-oath reports 1px borders instead.
		# Do not pass --force-windows-fullscreen: Steam's offscreen
		# CEF browser is created at INT_MIN with size 0x0, and
		# stretching that buffer to 1920x1080 is GPU garbage
		# (static) on RADV SI before steamui dies.
		exec /bin/gamescope --backend wayland -S fit \
			-W 1920 -H 1080 -w 1920 -h 1080 \
			--cursor-scale-height 1080 \
			--disable-color-management \
			--steam \
			-- "$0" "$@"
	fi
fi
if [ -n "${DISPLAY-}" ]; then
	export XDG_SESSION_TYPE=x11
	export SDL_VIDEODRIVER=x11
	export GDK_BACKEND=x11
	export QT_QPA_PLATFORM=xcb
	# Nested Xwayland: native xlib WSI. The FROG gamescope WSI layer
	# is 64-bit only; leaving ENABLE_GAMESCOPE_WSI set hides surface
	# extensions from 32-bit steamui. Pitcairn is SDR.
	export ENABLE_GAMESCOPE_WSI=0
	export ENABLE_HDR_WSI=0
	export DXVK_HDR=0
	if [ -n "${GAMESCOPE_WAYLAND_DISPLAY-}" ]; then
		# gamescope nest: Deck / gamepad UI (the chrome gamescope
		# actually speaks). Do not rewrite XDG_CURRENT_DESKTOP or
		# unset GAMESCOPE_WAYLAND_DISPLAY — those were forcing the
		# desktop library window that segfaults after login.
		# gamescope UpdateCompatEnvVars always sets HDR_SUPPORTED=1;
		# Pitcairn is SDR and HDR pass-through looks like static.
		export STEAM_GAMESCOPE_HDR_SUPPORTED=0
		export SteamDeck=1
		export STEAM_USE_GAMEPADUI=1
		export SteamTenfoot=1
		sudo -n mkdir -p /usr/bin/steamos-polkit-helpers 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/steam/libexec/steamos-polkit-helpers/steamos-devkit-mode \
			/usr/bin/steamos-polkit-helpers/steamos-devkit-mode 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/steam/libexec/steamos-polkit-helpers/jupiter-dock-updater \
			/usr/bin/steamos-polkit-helpers/jupiter-dock-updater 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/steam/libexec/steamos-select-branch \
			/usr/bin/steamos-select-branch 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/steam/libexec/lsb_release \
			/usr/bin/lsb_release 2>/dev/null || true
		sudo -n ln -sfn /oath/store/pkg/steam/libexec/timedatectl \
			/usr/bin/timedatectl 2>/dev/null || true
		case " $* " in
		*" -gamepadui "*|*" -steamdeck "*) ;;
		*) set -- -gamepadui -steamdeck "$@" ;;
		esac
	else
		# Rootful leftover only. gamescope's nested X is already a WM.
		if [ -x /oath/store/pkg/xwayland/libexec/xwayland-clip ]; then
			/oath/store/pkg/xwayland/libexec/xwayland-clip --daemon
		fi
		if [ -x /oath/store/pkg/xwayland/libexec/oath-xwm ]; then
			pidfile=/tmp/oath-xwm.pid
			old=$(cat "$pidfile" 2>/dev/null || true)
			if [ -z "$old" ] || ! kill -0 "$old" 2>/dev/null; then
				/oath/store/pkg/xwayland/libexec/oath-xwm >>/tmp/oath-xwm.log 2>&1 &
				echo $! >"$pidfile"
			fi
		fi
		export XDG_CURRENT_DESKTOP=Sola
		export XDG_SESSION_DESKTOP=Sola
		export SteamDeck=0
		export STEAM_USE_GAMEPADUI=0
		export SteamTenfoot=0
	fi
	# steam.sh execs the client after package extract. DEBUGGER is the
	# post-extract hook (STEAM_DEBUGGER=${DEBUGGER-}). Re-apply steamui
	# shims the updater just overwrote, then exec the real ELF.
	if [ -x /oath/store/pkg/steam/libexec/oath-steam-preexec ]; then
		export DEBUGGER=/oath/store/pkg/steam/libexec/oath-steam-preexec
	fi
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
It must not rewrite pkg:glibc (Ubuntu folded libresolv into libc; this
glibc still ships a separate libresolv that tmux NEEDs). srt-logger
gets libresolv from lib/srt. steamwebhelper skips pressure-vessel
(CLONE_NEWUSER is EPERM after PID 1 chroot) and runs on the host with
64-bit steamrt3 SONAMEs in lib64 (`--disable-gpu` on RADV SI).
steamui SDL display 0×0 is clamped; steamwebhelper `liboath-cefgeom`
clamps X11/xcb 0×0 creates. GAMESCOPE_VIEWPORT_SUPPORTED is faked as
0 (atom present) so steamui sizes the SDL window; value 1 took the
overlay path and left MainMenu 1×1 hidden. HDR atom stays 0.
liboath-glclass.so (64-bit LD_PRELOAD) redirects libGL/libEGL dlopen
to pkg:mesa so gldriverquery does not hit ubuntu12_32 ELFCLASS32.
liboath-peercred.so is dlmopen’d as a patched copy of steamui.so
(do not patchelf the live file). libexec/oath-lsof is the
lsof Steam's WebUITransport runs (`-P -F upnR -i TCP@…`). Removable.
PID 1 does not supervise Steam.
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
install_real_libresolv
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
# wlroots hardcodes /usr/bin/Xwayland; gamescope scripts use /usr/share/gamescope.
as_root ln -sfn /oath/store/pkg/xwayland/bin/Xwayland /usr/bin/Xwayland
as_root ln -sfn /oath/store/pkg/gamescope/share/gamescope /usr/share/gamescope

if [ "$(id -u)" = 0 ]; then
	oath apply pkg:xwayland pkg:gamescope pkg:steam
else
	sudo -n oath apply pkg:xwayland pkg:gamescope pkg:steam
fi

echo "==> sola launcher Steam"
# User catalog (settings/applications.json) plus live bus so Super+Space
# lists Steam without a shell restart. Built-ins stay in sola-shell.
home_cfg="${HOME:-/home}/.config/sola/shell"
mkdir -p "$home_cfg"
if [ ! -f "$home_cfg/applications.json" ]; then
	cat >"$home_cfg/applications.json" <<'JSON'
{
  "apps": [
    {
      "app_id": "steam",
      "label": "Steam",
      "command": "/bin/steam",
      "icon": "lucide/gamepad-2"
    }
  ]
}
JSON
fi
if command -v solactl >/dev/null 2>&1; then
	solactl emit Application '{"app_id":"steam","label":"Steam","command":"/bin/steam","icon":"lucide/gamepad-2"}' \
		2>/dev/null || true
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
