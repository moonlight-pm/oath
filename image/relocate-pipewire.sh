#!/usr/bin/env bash
# Relocate glibc PipeWire + WirePlumber + ALSA + libpulse into $out
# for packing as pkg:pipewire. Guest rpath includes pkg:glibc and
# pkg:river (libudev-zero — no udevd). SPA/ALSA plugins stay next to
# the daemon; wrappers set the module/config dirs.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_river=/oath/store/pkg/river/lib
guest_pw=/oath/store/pkg/pipewire/lib
rpath="$guest_glibc:$guest_pw:$guest_river"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

PIPEWIRE=${PIPEWIRE:?}
WIREPLUMBER=${WIREPLUMBER:?}
ALSA_LIB=${ALSA_LIB:?}
LIBPULSE=${LIBPULSE:?}

if [[ -e $out ]]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec" \
	"$out/lib/spa-0.2" "$out/lib/pipewire-0.3" "$out/lib/wireplumber-0.5" \
	"$out/lib/pulseaudio" "$out/share"

is_glibc() {
	case "$(basename "$1")" in
	ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
	libgcc_s.so*|libstdc++.so*)
		return 0
		;;
	*) return 1 ;;
	esac
}

# libsystemd is the daemon client. Keep libudev.so — spa-alsa enumerates
# cards from sysfs without udevd. (libudev-zero only covers input/DRM.)
is_skip() {
	case "$(basename "$1")" in
	libsystemd.so*|libsystemd-shared*)
		return 0
		;;
	*) return 1 ;;
	esac
}

declare -A SEEN=()
declare -A SONAME=()
queue=()
loader=""

enqueue() {
	local f=$1
	[[ -e $f ]] || return 0
	local real
	real=$(readlink -f "$f")
	[[ -n ${SEEN[$real]+x} ]] && return 0
	SEEN[$real]=1
	SONAME[$real]=$(basename "$f")
	queue+=("$real")
}

copy_tree_dir() {
	local src=$1 dest=$2
	[[ -d $src ]] || return 0
	mkdir -p "$dest"
	cp -a "$src/." "$dest/"
	chmod -R u+w "$dest" 2>/dev/null || true
}

for b in pipewire pipewire-pulse pw-dump pw-cat pw-cli; do
	[[ -f $PIPEWIRE/bin/$b ]] || { echo "relocate-pipewire: missing $PIPEWIRE/bin/$b" >&2; exit 1; }
	cp -a "$PIPEWIRE/bin/$b" "$out/libexec/$b"
	chmod u+w "$out/libexec/$b"
	enqueue "$out/libexec/$b"
done
# pw-play / pw-record are the same ELF as pw-cat (argv0).
ln -sf pw-cat "$out/libexec/pw-play"
ln -sf pw-cat "$out/libexec/pw-record"

for b in wireplumber wpctl; do
	[[ -f $WIREPLUMBER/bin/$b ]] || { echo "relocate-pipewire: missing $WIREPLUMBER/bin/$b" >&2; exit 1; }
	cp -a "$WIREPLUMBER/bin/$b" "$out/libexec/$b"
	chmod u+w "$out/libexec/$b"
	enqueue "$out/libexec/$b"
done

copy_tree_dir "$PIPEWIRE/lib/pipewire-0.3" "$out/lib/pipewire-0.3"
rm -rf "$out/lib/pipewire-0.3/v4l2"
copy_tree_dir "$WIREPLUMBER/lib/wireplumber-0.5" "$out/lib/wireplumber-0.5"

for spa in support alsa audioconvert audiomixer control; do
	copy_tree_dir "$PIPEWIRE/lib/spa-0.2/$spa" "$out/lib/spa-0.2/$spa"
done
if [[ -f $PIPEWIRE/lib/spa-0.2/libspa.so ]]; then
	cp -a "$PIPEWIRE/lib/spa-0.2/libspa.so" "$out/lib/spa-0.2/libspa.so"
	chmod u+w "$out/lib/spa-0.2/libspa.so"
fi

copy_tree_dir "$PIPEWIRE/share/pipewire" "$out/share/pipewire"
copy_tree_dir "$PIPEWIRE/share/alsa-card-profile" "$out/share/alsa-card-profile"
copy_tree_dir "$PIPEWIRE/share/alsa" "$out/share/alsa"
copy_tree_dir "$ALSA_LIB/share/alsa" "$out/share/alsa"
copy_tree_dir "$WIREPLUMBER/share/wireplumber" "$out/share/wireplumber"

if [[ -d $PIPEWIRE/lib/alsa-lib ]]; then
	copy_tree_dir "$PIPEWIRE/lib/alsa-lib" "$out/lib/alsa-lib"
fi
if [[ -d $ALSA_LIB/lib ]]; then
	for so in "$ALSA_LIB"/lib/libasound.so*; do
		[[ -e $so ]] || continue
		cp -a "$so" "$out/lib/$(basename "$so")"
		chmod u+w "$out/lib/$(basename "$so")" 2>/dev/null || true
		enqueue "$so"
	done
fi
if [[ -d $LIBPULSE/lib ]]; then
	for so in "$LIBPULSE"/lib/libpulse.so* "$LIBPULSE"/lib/libpulse-simple.so*; do
		[[ -e $so ]] || continue
		cp -a "$so" "$out/lib/$(basename "$so")"
		chmod u+w "$out/lib/$(basename "$so")" 2>/dev/null || true
		enqueue "$so"
	done
	if [[ -d $LIBPULSE/lib/pulseaudio ]]; then
		copy_tree_dir "$LIBPULSE/lib/pulseaudio" "$out/lib/pulseaudio"
	fi
fi

while IFS= read -r -d '' f; do
	if file -b "$f" | grep -q ELF; then
		enqueue "$f"
	fi
done < <(find "$out/lib" "$out/libexec" -type f -print0)

i=0
while [[ $i -lt ${#queue[@]} ]]; do
	f=${queue[$i]}
	i=$((i + 1))
	if [[ -z $loader ]] && file -b "$f" | grep -q ELF; then
		if loader=$(patchelf --print-interpreter "$f" 2>/dev/null); then
			enqueue "$loader"
		else
			loader=""
		fi
	fi
	if [[ -n $loader && -x $loader ]]; then
		while read -r dep; do
			[[ -e $dep ]] && enqueue "$dep"
		done < <("$loader" --list "$f" 2>/dev/null | awk '/=> \// {print $3} /^\//{print $1}')
	fi
done

for f in "${!SEEN[@]}"; do
	name=${SONAME[$f]:-$(basename "$f")}
	if is_glibc "$name" || is_skip "$name"; then
		continue
	fi
	if [[ $f == "$out"/* ]]; then
		continue
	fi
	d="$out/lib/$name"
	mkdir -p "$(dirname "$d")"
	cp -a "$f" "$d"
	chmod u+w "$d" 2>/dev/null || true
done

find "$out/libexec" "$out/lib" -type f | while read -r f; do
	file -b "$f" | grep -q ELF || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp_guest" "$f" || true
	fi
	patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
done

mkdir -p "$out/share/wireplumber/wireplumber.conf.d"
cat >"$out/share/wireplumber/wireplumber.conf.d/00-oath-audio.conf" <<'EOF'
wireplumber.profiles.main = {
  hardware.bluetooth = disabled
  hardware.video-capture = disabled
}
EOF

# No udevd: spa-alsa udev enum sees no cards. Pin the Intel PCH analog
# PCM so the menubar has a real sink (HDMI heads can be added later).
mkdir -p "$out/share/pipewire/pipewire.conf.d"
cat >"$out/share/pipewire/pipewire.conf.d/99-oath-alsa.conf" <<'EOF'
context.objects = [
  { factory = adapter
    args = {
      factory.name     = api.alsa.pcm.sink
      node.name        = "alsa-sink-pch"
      node.description = "Built-in Audio"
      media.class      = "Audio/Sink"
      api.alsa.path    = "hw:0,0"
      audio.rate       = 48000
      audio.channels   = 2
      audio.position   = "FL,FR"
    }
  }
  { factory = adapter
    args = {
      factory.name     = api.alsa.pcm.source
      node.name        = "alsa-source-pch"
      node.description = "Built-in Audio"
      media.class      = "Audio/Source"
      api.alsa.path    = "hw:0,0"
      audio.rate       = 48000
      audio.channels   = 2
      audio.position   = "FL,FR"
    }
  }
]
EOF

write_wrap() {
	local name=$1
	local extra=${2:-}
	{
		echo '#!/bin/sh'
		echo 'root=/oath/store/pkg/pipewire'
		echo 'export SPA_PLUGIN_DIR="${SPA_PLUGIN_DIR:-$root/lib/spa-0.2}"'
		echo 'export PIPEWIRE_MODULE_DIR="${PIPEWIRE_MODULE_DIR:-$root/lib/pipewire-0.3}"'
		echo 'export PIPEWIRE_CONFIG_DIR="${PIPEWIRE_CONFIG_DIR:-$root/share/pipewire}"'
		echo 'export PIPEWIRE_RUNTIME_DIR="${PIPEWIRE_RUNTIME_DIR:-${XDG_RUNTIME_DIR:-/run/user/1}}"'
		echo 'export WIREPLUMBER_CONFIG_DIR="${WIREPLUMBER_CONFIG_DIR:-$root/share/wireplumber}"'
		echo 'export WIREPLUMBER_MODULE_DIR="${WIREPLUMBER_MODULE_DIR:-$root/lib/wireplumber-0.5}"'
		echo 'export WIREPLUMBER_DATA_DIR="${WIREPLUMBER_DATA_DIR:-$root/share/wireplumber}"'
		echo 'export ALSA_CONFIG_PATH="${ALSA_CONFIG_PATH:-$root/share/alsa/alsa.conf}"'
		echo 'export GIO_MODULE_DIR="${GIO_MODULE_DIR:-}"'
		if [[ -n $extra ]]; then
			echo "exec \$root/libexec/$name $extra \"\$@\""
		else
			echo "exec \$root/libexec/$name \"\$@\""
		fi
	} >"$out/bin/$name"
	chmod +x "$out/bin/$name"
}

write_wrap pipewire
write_wrap pipewire-pulse
write_wrap pw-dump
write_wrap pw-cat
write_wrap pw-cli
write_wrap wpctl
write_wrap wireplumber "--profile main-embedded"

# Preserve argv0 so pw-cat knows play vs record.
for alias in pw-play pw-record; do
	{
		echo '#!/bin/sh'
		echo 'root=/oath/store/pkg/pipewire'
		echo 'export SPA_PLUGIN_DIR="${SPA_PLUGIN_DIR:-$root/lib/spa-0.2}"'
		echo 'export PIPEWIRE_MODULE_DIR="${PIPEWIRE_MODULE_DIR:-$root/lib/pipewire-0.3}"'
		echo 'export PIPEWIRE_CONFIG_DIR="${PIPEWIRE_CONFIG_DIR:-$root/share/pipewire}"'
		echo 'export PIPEWIRE_RUNTIME_DIR="${PIPEWIRE_RUNTIME_DIR:-${XDG_RUNTIME_DIR:-/run/user/1}}"'
		echo 'export WIREPLUMBER_CONFIG_DIR="${WIREPLUMBER_CONFIG_DIR:-$root/share/wireplumber}"'
		echo 'export WIREPLUMBER_MODULE_DIR="${WIREPLUMBER_MODULE_DIR:-$root/lib/wireplumber-0.5}"'
		echo 'export WIREPLUMBER_DATA_DIR="${WIREPLUMBER_DATA_DIR:-$root/share/wireplumber}"'
		echo 'export ALSA_CONFIG_PATH="${ALSA_CONFIG_PATH:-$root/share/alsa/alsa.conf}"'
		echo 'export GIO_MODULE_DIR="${GIO_MODULE_DIR:-}"'
		echo "exec -a $alias \$root/libexec/pw-cat \"\$@\""
	} >"$out/bin/$alias"
	chmod +x "$out/bin/$alias"
done
