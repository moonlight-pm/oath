#!/usr/bin/env bash
# Relocate a glibc River (and its ELF deps) into $out/{glibc,river}
# for packing as pkg:glibc + pkg:river. Guest paths are under /oath/store.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_river=/oath/store/pkg/river/lib
rpath="$guest_glibc:$guest_river"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

mkdir -p "$out/glibc/lib" "$out/glibc/bin" "$out/river/lib" "$out/river/bin" \
  "$out/river/libexec" "$out/river/lib/dri" "$out/river/lib/gbm" \
  "$out/river/share/glvnd/egl_vendor.d" "$out/river/share/X11"

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

dest_for() {
  if is_glibc "$1"; then
    echo "$out/glibc/lib/$(basename "$1")"
  else
    echo "$out/river/lib/$(basename "$1")"
  fi
}

loader=""
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

enqueue "$RIVER/bin/river"
if [[ -n ${SEATD:-} ]]; then
  enqueue "$SEATD/bin/seatd"
  enqueue "$SEATD/bin/seatd-launch"
fi

# Mesa / glvnd pieces wlroots dlopens (not always in NEEDED).
if [[ -n ${MESA:-} ]]; then
  enqueue "$MESA/lib/dri/libdril_dri.so" || true
  enqueue "$MESA/lib/libgallium-26.1.0.so" || true
  for g in "$MESA"/lib/libgallium*.so; do
    [[ -e $g ]] && enqueue "$g"
  done
  enqueue "$MESA/lib/libEGL_mesa.so.0" || true
  enqueue "$MESA/lib/gbm/dri_gbm.so" || true
  enqueue "$MESA/lib/libvulkan_virtio.so" || true
fi
if [[ -n ${LIBGLVND:-} ]]; then
  enqueue "$LIBGLVND/lib/libGLdispatch.so.0" || true
  enqueue "$LIBGLVND/lib/libEGL.so.1" || true
  enqueue "$LIBGLVND/lib/libGLESv2.so.2" || true
fi

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
    d="$out/glibc/lib/$name"
  else
    d="$out/river/lib/$name"
  fi
  mkdir -p "$(dirname "$d")"
  cp -a "$f" "$d"
  chmod u+w "$d" 2>/dev/null || true
done

# systemd libudev needs udevd. libudev-zero does not (T22).
if [[ -n ${LIBUDEV_ZERO:-} && -e $LIBUDEV_ZERO/lib/libudev.so.1 ]]; then
  rm -f "$out/river/lib"/libudev.so*
  cp -a "$LIBUDEV_ZERO/lib/libudev.so.1" "$out/river/lib/libudev.so.1"
  chmod u+w "$out/river/lib/libudev.so.1" 2>/dev/null || true
fi

if [[ -n ${LIBINPUT_SHARE:-} && -d $LIBINPUT_SHARE ]]; then
  mkdir -p "$out/river/share/libinput"
  cp -aL "$LIBINPUT_SHARE/." "$out/river/share/libinput/"
fi

# DRI / GBM names Mesa looks up.
if [[ -n ${MESA:-} ]]; then
  if [[ -e $out/river/lib/libdril_dri.so ]]; then
    ln -sf ../libdril_dri.so "$out/river/lib/dri/virtio_gpu_dri.so"
    ln -sf ../libdril_dri.so "$out/river/lib/dri/kms_swrast_dri.so"
    ln -sf ../libdril_dri.so "$out/river/lib/dri/swrast_dri.so"
    ln -sf ../libdril_dri.so "$out/river/lib/dri/libdril_dri.so"
  fi
  if [[ -e $out/river/lib/dri_gbm.so ]]; then
    ln -sf ../dri_gbm.so "$out/river/lib/gbm/dri_gbm.so"
  fi
  cat >"$out/river/share/glvnd/egl_vendor.d/50_mesa.json" <<EOF
{
    "file_format_version" : "1.0.0",
    "ICD" : {
        "library_path" : "/oath/store/pkg/river/lib/libEGL_mesa.so.0"
    }
}
EOF
fi

if [[ -n ${XKB:-} && -d $XKB/share/X11/xkb ]]; then
  mkdir -p "$out/river/share/X11/xkb"
  cp -aL "$XKB/share/X11/xkb/." "$out/river/share/X11/xkb/"
fi

# ld-linux name the guest interpreter path expects.
if [[ -n $loader ]]; then
  base=$(basename "$loader")
  if [[ $base != ld-linux-x86-64.so.2 ]]; then
    ln -sfn "$base" "$out/glibc/lib/ld-linux-x86-64.so.2"
  fi
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

cp -a "$RIVER/bin/river" "$out/river/libexec/river"
chmod u+w "$out/river/libexec/river"
chmod +x "$out/river/libexec/river"
if patchelf --print-interpreter "$out/river/libexec/river" >/dev/null 2>&1; then
  patchelf --set-interpreter "$interp_guest" "$out/river/libexec/river"
fi
patchelf --set-rpath "$rpath" "$out/river/libexec/river"

for b in seatd seatd-launch; do
  if [[ -e $out/river/lib/$b ]]; then
    mv "$out/river/lib/$b" "$out/river/libexec/$b"
    chmod u+w "$out/river/libexec/$b"
    chmod +x "$out/river/libexec/$b"
    if patchelf --print-interpreter "$out/river/libexec/$b" >/dev/null 2>&1; then
      patchelf --set-interpreter "$interp_guest" "$out/river/libexec/$b"
    fi
    patchelf --set-rpath "$rpath" "$out/river/libexec/$b"
  fi
done

cat >"$out/river/bin/river" <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME=/root
export XDG_RUNTIME_DIR=/run/user/0
/bin/mkdir -p "$XDG_RUNTIME_DIR"
/bin/chmod 700 "$XDG_RUNTIME_DIR"
export LIBSEAT_BACKEND=seatd
export XDG_SESSION_TYPE=tty
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export XKB_CONFIG_ROOT=/oath/store/pkg/river/share/X11/xkb
# virtio-gpu-pci is 2D; gles2 needs virgl. Same as Sola's QEMU session.
export WLR_RENDERER=pixman
export WLR_RENDERER_ALLOW_SOFTWARE=1
# No udevd: libudev-zero + wlroots path fallback on /dev/input/event*.
export LIBINPUT_QUIRKS_DIR=/oath/store/pkg/river/share/libinput
unset WAYLAND_DISPLAY
unset DISPLAY
exec /oath/store/pkg/river/libexec/river -log-level info -c : >>/oath/log/river.log 2>&1
WRAP
chmod +x "$out/river/bin/river"
if [[ -x $out/river/libexec/seatd ]]; then
  cp -a "$out/river/libexec/seatd" "$out/river/bin/seatd"
  chmod +x "$out/river/bin/seatd"
fi
[[ -x $out/river/bin/seatd ]] || {
  echo "relocate-river: missing bin/seatd" >&2
  exit 1
}
