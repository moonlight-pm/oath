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
  cp -a "$XKB/share/X11/xkb" "$out/river/share/X11/xkb"
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

cat >"$out/river/bin/river" <<'WRAP'
#!/bin/sh
export XDG_RUNTIME_DIR=/run/user/0
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export HOME=/root
export LIBSEAT_BACKEND=builtin
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export XKB_CONFIG_ROOT=/oath/store/pkg/river/share/X11/xkb
export WLR_RENDERER=gles2
unset WAYLAND_DISPLAY
unset DISPLAY
exec /oath/store/pkg/river/libexec/river -log-level info -c :
WRAP
chmod +x "$out/river/bin/river"
