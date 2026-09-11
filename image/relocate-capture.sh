#!/usr/bin/env bash
# Relocate grim, slurp, hyprpicker, wl-clipboard, and jq for Omarchy capture.
# Guest interpreter is pkg:glibc. Farm name is pkg:grim.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_lib=/oath/store/pkg/grim/lib
rpath="$guest_glibc:$guest_lib"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

if [[ -e $out ]]; then
  chmod -R u+w "$out" 2>/dev/null || true
  rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec"

is_glibc() {
  case "$(basename "$1")" in
    ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
    libresolv.so*|libutil.so*|libcrypt.so*|libnss_*|libgcc_s.so*| \
    libstdc++.so*|libssp.so*|libthread_db.so*|libanl.so*|libcidn.so*)
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

pick_elf() {
  local dir=$1 name=$2
  local c
  for c in "$dir/bin/$name" "$dir/bin/.$name-wrapped" "$dir/bin/.$name-wrapped-wrapped"; do
    if [[ -e $c ]] && file -b "$c" | grep -q ELF; then
      printf '%s' "$c"
      return 0
    fi
  done
  return 1
}

bins=()
add_bin() {
  local dir=$1 name=$2
  local elf
  elf=$(pick_elf "$dir" "$name") || {
    echo "relocate-capture: no $name ELF under $dir/bin" >&2
    ls -l "$dir/bin" >&2 || true
    exit 1
  }
  enqueue "$elf"
  bins+=("$name:$elf")
}

GRIM=${GRIM:?}
SLURP=${SLURP:?}
HYPRPICKER=${HYPRPICKER:?}
WLCLIP=${WLCLIP:?}
JQ=${JQ:?}

add_bin "$GRIM" grim
add_bin "$SLURP" slurp
add_bin "$HYPRPICKER" hyprpicker
add_bin "$WLCLIP" wl-copy
# wl-paste is the same package; optional.
if pick_elf "$WLCLIP" wl-paste >/dev/null; then
  add_bin "$WLCLIP" wl-paste
fi
add_bin "$JQ" jq

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
  if is_glibc "$name"; then
    continue
  fi
  d="$out/lib/$name"
  mkdir -p "$(dirname "$d")"
  cp -a "$f" "$d"
  chmod u+w "$d" 2>/dev/null || true
done

for entry in "${bins[@]}"; do
  name=${entry%%:*}
  elf=${entry#*:}
  srcname=$(basename "$elf")
  mv "$out/lib/$srcname" "$out/libexec/$name"
  chmod u+w "$out/libexec/$name"
  chmod +x "$out/libexec/$name"
done

find "$out" -type f | while read -r f; do
  file -b "$f" | grep -q ELF || continue
  case "$(basename "$f")" in
    ld-linux*) continue ;;
  esac
  chmod u+w "$f" || true
  if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$f" || true
  fi
  patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
done

for entry in "${bins[@]}"; do
  name=${entry%%:*}
  cat >"$out/bin/$name" <<WRAP
#!/bin/sh
export PATH="/lib/oath:/oath/store/pkg/omarchy/bin:/oath/store/pkg/grim/bin:/bin"
export HOME="\${HOME:-/home}"
export XDG_RUNTIME_DIR="\${XDG_RUNTIME_DIR:-/run/user/1}"
/bin/mkdir -p "\$XDG_RUNTIME_DIR"
export LANG="\${LANG:-C.UTF-8}"
export LC_ALL="\${LC_ALL:-C.UTF-8}"
export LOCALE_ARCHIVE="\${LOCALE_ARCHIVE:-/oath/store/pkg/sola/lib/locale/locale-archive}"
export XKB_CONFIG_ROOT="\${XKB_CONFIG_ROOT:-/oath/store/pkg/hyprland/share/X11/xkb}"
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
if [ -z "\${WAYLAND_DISPLAY:-}" ]; then
	for s in "\$XDG_RUNTIME_DIR"/wayland-*; do
		[ -S "\$s" ] || continue
		case "\$s" in
		*.lock) continue ;;
		esac
		WAYLAND_DISPLAY=\$(basename "\$s")
		export WAYLAND_DISPLAY
		break
	done
fi
exec /oath/store/pkg/grim/libexec/$name "\$@"
WRAP
  chmod +x "$out/bin/$name"
done

for entry in "${bins[@]}"; do
  name=${entry%%:*}
  [[ -x $out/libexec/$name ]] || {
    echo "relocate-capture: missing libexec/$name" >&2
    exit 1
  }
done
