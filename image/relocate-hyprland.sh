#!/usr/bin/env bash
# Relocate a glibc Hyprland (and ELF deps) into $out for pkg:hyprland.
# Guest interpreter is pkg:glibc (already packed with River).
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_hypr=/oath/store/pkg/hyprland/lib
rpath="$guest_glibc:$guest_hypr"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

mkdir -p "$out/lib" "$out/bin" "$out/libexec" "$out/lib/dri" "$out/lib/gbm" \
  "$out/share/glvnd/egl_vendor.d" "$out/share/X11"

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

hypr_bin=
for c in "$HYPRLAND/bin/Hyprland" "$HYPRLAND/bin/.Hyprland-wrapped" "$HYPRLAND/bin/hyprland"; do
  if [[ -e $c ]]; then
    if file -b "$c" | grep -q ELF; then
      hypr_bin=$c
      break
    fi
  fi
done
[[ -n $hypr_bin ]] || {
  echo "relocate-hyprland: no Hyprland ELF under $HYPRLAND/bin" >&2
  ls -l "$HYPRLAND/bin" >&2 || true
  exit 1
}
enqueue "$hypr_bin"
if [[ -e $HYPRLAND/bin/hyprctl ]] && file -b "$HYPRLAND/bin/hyprctl" | grep -q ELF; then
  enqueue "$HYPRLAND/bin/hyprctl"
fi

if [[ -n ${MESA:-} ]]; then
  enqueue "$MESA/lib/dri/libdril_dri.so" || true
  for g in "$MESA"/lib/libgallium*.so; do
    [[ -e $g ]] && enqueue "$g"
  done
  enqueue "$MESA/lib/libEGL_mesa.so.0" || true
  enqueue "$MESA/lib/gbm/dri_gbm.so" || true
  enqueue "$MESA/lib/libvulkan_virtio.so" || true
  enqueue "$MESA/lib/libvulkan_radeon.so" || true
fi
if [[ -n ${LIBGLVND:-} ]]; then
  enqueue "$LIBGLVND/lib/libGLdispatch.so.0" || true
  enqueue "$LIBGLVND/lib/libEGL.so.1" || true
  enqueue "$LIBGLVND/lib/libGLESv2.so.2" || true
fi

loader=""
i=0
while [[ $i -lt ${#queue[@]} ]]; do
  f=${queue[$i]}
  i=$((i + 1))
  if [[ -z $loader ]] && file -b "$f" | grep -q 'ELF'; then
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

if [[ -n ${LIBUDEV_ZERO:-} && -e $LIBUDEV_ZERO/lib/libudev.so.1 ]]; then
  rm -f "$out/lib"/libudev.so*
  cp -a "$LIBUDEV_ZERO/lib/libudev.so.1" "$out/lib/libudev.so.1"
  chmod u+w "$out/lib/libudev.so.1" 2>/dev/null || true
fi

if [[ -n ${LIBINPUT_SHARE:-} && -d $LIBINPUT_SHARE ]]; then
  mkdir -p "$out/share/libinput"
  cp -aL "$LIBINPUT_SHARE/." "$out/share/libinput/"
fi

if [[ -n ${MESA:-} ]]; then
  # Absolute guest paths: busybox tar strips leading `../` from symlink
  # targets, which turns `../dri_gbm.so` into a self-loop.
  if [[ -e $out/lib/libdril_dri.so ]]; then
    for n in virtio_gpu_dri.so kms_swrast_dri.so swrast_dri.so libdril_dri.so radeonsi_dri.so radeon_dri.so; do
      ln -sfn /oath/store/pkg/hyprland/lib/libdril_dri.so "$out/lib/dri/$n"
    done
  fi
  if [[ -e $out/lib/dri_gbm.so ]]; then
    ln -sfn /oath/store/pkg/hyprland/lib/dri_gbm.so "$out/lib/gbm/dri_gbm.so"
  fi
  cat >"$out/share/glvnd/egl_vendor.d/50_mesa.json" <<EOF
{
    "file_format_version" : "1.0.0",
    "ICD" : {
        "library_path" : "/oath/store/pkg/hyprland/lib/libEGL_mesa.so.0"
    }
}
EOF
fi

if [[ -n ${XKB:-} && -d $XKB/share/X11/xkb ]]; then
  mkdir -p "$out/share/X11/xkb"
  cp -aL "$XKB/share/X11/xkb/." "$out/share/X11/xkb/"
fi

hypr_name=$(basename "$hypr_bin")
cp -a "$out/lib/$hypr_name" "$out/libexec/Hyprland"
chmod u+w "$out/libexec/Hyprland"
chmod +x "$out/libexec/Hyprland"
if patchelf --print-interpreter "$out/libexec/Hyprland" >/dev/null 2>&1; then
  patchelf --set-interpreter "$interp_guest" "$out/libexec/Hyprland"
fi
patchelf --set-rpath "$rpath" "$out/libexec/Hyprland"
rm -f "$out/lib/$hypr_name"

if [[ -e $out/lib/hyprctl ]]; then
  mv "$out/lib/hyprctl" "$out/libexec/hyprctl"
  chmod u+w "$out/libexec/hyprctl"
  chmod +x "$out/libexec/hyprctl"
  if patchelf --print-interpreter "$out/libexec/hyprctl" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$out/libexec/hyprctl"
  fi
  patchelf --set-rpath "$rpath" "$out/libexec/hyprctl"
  ln -sfn /oath/store/pkg/hyprland/libexec/hyprctl "$out/bin/hyprctl"
fi

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

cat >"$out/bin/hyprland" <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
/bin/mkdir -p "$XDG_RUNTIME_DIR"
/bin/chmod 700 "$XDG_RUNTIME_DIR"
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"
export LOCALE_ARCHIVE="${LOCALE_ARCHIVE:-/oath/store/pkg/sola/lib/locale/locale-archive}"
export LIBSEAT_BACKEND=seatd
export XDG_SESSION_TYPE=tty
export LIBGL_DRIVERS_PATH=/oath/store/pkg/hyprland/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/hyprland/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/hyprland/share/glvnd/egl_vendor.d/50_mesa.json
export XKB_CONFIG_ROOT=/oath/store/pkg/hyprland/share/X11/xkb
export LIBINPUT_QUIRKS_DIR=/oath/store/pkg/hyprland/share/libinput
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
unset WAYLAND_DISPLAY
unset DISPLAY
card=
for c in /sys/class/drm/card[0-9]; do
	[ -d "$c" ] || continue
	name=$(basename "$c")
	case "$name" in
	card[0-9]) ;;
	*) continue ;;
	esac
	for conn in "$c"/"$name"-*; do
		[ -f "$conn/status" ] || continue
		if [ "$(cat "$conn/status")" = connected ]; then
			n=${name#card}
			if [ -e "/dev/dri/card$n" ]; then
				card=/dev/dri/card$n
				break
			fi
		fi
	done
	[ -n "$card" ] && break
done
[ -n "$card" ] && export AQ_DRM_DEVICES=$card
export HYPRLAND_NO_SD_NOTIFY=1
exec /oath/store/pkg/hyprland/libexec/Hyprland -c /lib/oath/hyprland-boot.conf >>/oath/log/hyprland.log 2>&1
WRAP
chmod +x "$out/bin/hyprland"
[[ -x $out/libexec/Hyprland ]] || {
  echo "relocate-hyprland: missing libexec/Hyprland" >&2
  exit 1
}
