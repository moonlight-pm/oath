#!/bin/bash
# Live-install pkg:xwayland, pkg:gamescope, pkg:steam and sola-arcade on this
# Oath box. Ubuntu questing debs + Steam bootstrap + Debian i386 libc.
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
export VK_LAYER_PATH="${VK_LAYER_PATH:-/oath/store/pkg/gamescope/share}"
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
export PATH=/bin
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
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
# zenity stub — Steam only uses it for dialogs.
cat >"$stagedir/steam/bin/zenity" <<'Z'
#!/bin/sh
echo "zenity-stub: $*" >&2
exit 0
Z
chmod 755 "$stagedir/steam/bin/zenity"
cat >"$stagedir/steam/bin/steam" <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
# 32-bit steam ELF wants /lib/ld-linux.so.2. Point a live node at our copy.
if [ ! -e /lib/ld-linux.so.2 ] && [ -f /oath/store/pkg/steam/lib32/ld-linux.so.2 ]; then
	sudo -n ln -sfn /oath/store/pkg/steam/lib32/ld-linux.so.2 /lib/ld-linux.so.2 2>/dev/null || true
fi
export LD_LIBRARY_PATH="/oath/store/pkg/steam/lib32:${LD_LIBRARY_PATH:-}"
mkdir -p "$HOME/.steam" "$XDG_DATA_HOME/Steam" /tmp/fontconfig
# Valve's launcher is bash.
exec /bin/bash /oath/store/pkg/steam/libexec/bin_steam.sh "$@"
WRAP
chmod 755 "$stagedir/steam/bin/steam"
cat >"$stagedir/steam/INDEX.md" <<'EOF'
# pkg:steam

Valve steam-launcher (bootstrap tarball + bin_steam.sh) plus a 32-bit
glibc loader for ubuntu12_32/steam. User state is ~/.steam and
~/.local/share/Steam. Removable. PID 1 does not supervise Steam.
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
install_store steam "$stagedir/steam"

# /bin/steam must not fight an existing name.
for n in Xwayland gamescope steam zenity; do
	if [ -e /bin/$n ] && [ ! -L /bin/$n ]; then
		as_root rm -f /bin/$n
	fi
done
# 32-bit loader at the path the steam ELF encodes.
if [ -f "$store/steam/lib32/ld-linux.so.2" ]; then
	as_root mkdir -p /lib
	as_root ln -sfn /oath/store/pkg/steam/lib32/ld-linux.so.2 /lib/ld-linux.so.2
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
