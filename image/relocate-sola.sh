#!/usr/bin/env bash
# Relocate glibc-linked Sola session ELFs + dlopen deps into $out
# for packing as pkg:sola. Guest rpath includes pkg:glibc and pkg:river.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_river=/oath/store/pkg/river/lib
guest_sola=/oath/store/pkg/sola/lib
rpath="$guest_glibc:$guest_river:$guest_sola"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

mkdir -p "$out/lib" "$out/bin" "$out/libexec" \
  "$out/share/fonts" "$out/share/icons" "$out/share/cursors" "$out/etc/fonts"

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

bins=(sola-bus sola-call sola-river sola-shell sola-session)
for b in "${bins[@]}"; do
  src="${SOLA_BINS:?}/$b"
  [[ -f $src ]] || { echo "relocate-sola: missing $src" >&2; exit 1; }
  enqueue "$src"
done

enqueue_glob() {
  local g
  for g in "$@"; do
    [[ -e $g ]] || continue
    enqueue "$g"
  done
}

if [[ -n ${WAYLAND:-} ]]; then
  enqueue_glob "$WAYLAND"/lib/libwayland-client.so*
  enqueue_glob "$WAYLAND"/lib/libwayland-cursor.so*
  enqueue_glob "$WAYLAND"/lib/libwayland-egl.so*
fi
if [[ -n ${XKBCOMMON:-} ]]; then
  enqueue_glob "$XKBCOMMON"/lib/libxkbcommon.so*
fi
if [[ -n ${LIBFFI:-} ]]; then
  enqueue_glob "$LIBFFI"/lib/libffi.so*
fi
if [[ -n ${LIBGLVND:-} ]]; then
  enqueue_glob "$LIBGLVND"/lib/libGLdispatch.so*
  enqueue_glob "$LIBGLVND"/lib/libEGL.so*
  enqueue_glob "$LIBGLVND"/lib/libGLESv2.so*
  enqueue_glob "$LIBGLVND"/lib/libGL.so*
fi
if [[ -n ${VULKAN_LOADER:-} ]]; then
  enqueue_glob "$VULKAN_LOADER"/lib/libvulkan.so*
fi
if [[ -n ${FONTCONFIG:-} ]]; then
  enqueue_glob "$FONTCONFIG"/lib/libfontconfig.so*
fi
if [[ -n ${FREETYPE:-} ]]; then
  enqueue_glob "$FREETYPE"/lib/libfreetype.so*
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
    continue
  fi
  case "$name" in
    sola-bus|sola-call|sola-river|sola-shell|sola-session) continue ;;
  esac
  d="$out/lib/$name"
  cp -a "$f" "$d"
  chmod u+w "$d" 2>/dev/null || true
done

# dlopen looks up SONAME (libwayland-client.so.0), not the real basename.
for f in "$out"/lib/*; do
  [[ -f $f && ! -L $f ]] || continue
  soname=$(patchelf --print-soname "$f" 2>/dev/null || true)
  [[ -n $soname ]] || continue
  if [[ $soname != "$(basename "$f")" ]]; then
    ln -sfn "$(basename "$f")" "$out/lib/$soname"
  fi
done

# First-party pack icons (flower / pillars) from the Sola source tree.
if [[ -n ${SOLA_SRC:-} && -d $SOLA_SRC/crates/sola-assets/icons ]]; then
  cp -a "$SOLA_SRC/crates/sola-assets/icons/." "$out/share/icons/"
fi
# Host Sola share: lucide + McMojave (not in git).
if [[ -n ${SOLA_SHARE:-} ]]; then
  if [[ -d $SOLA_SHARE/icons/lucide ]]; then
    rm -rf "$out/share/icons/lucide"
    cp -a "$SOLA_SHARE/icons/lucide" "$out/share/icons/lucide"
  fi
  if [[ -d $SOLA_SHARE/cursors/McMojave ]]; then
    rm -rf "$out/share/cursors/McMojave"
    cp -a "$SOLA_SHARE/cursors/McMojave" "$out/share/cursors/McMojave"
  fi
fi

if [[ -n ${INTER:-} ]]; then
  if [[ -d $INTER/share/fonts ]]; then
    find "$INTER/share/fonts" -type f \( -name '*.ttf' -o -name '*.otf' -o -name '*.ttc' \) \
      -exec cp -a {} "$out/share/fonts/" \;
  fi
fi

cat >"$out/etc/fonts/fonts.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/oath/store/pkg/sola/share/fonts</dir>
  <cachedir>/tmp/fontconfig</cachedir>
</fontconfig>
EOF

find "$out/lib" -type f 2>/dev/null | while read -r f; do
  file -b "$f" | grep -q ELF || continue
  chmod u+w "$f" || true
  if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$f" || true
  fi
  patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
done

for b in "${bins[@]}"; do
  src="$SOLA_BINS/$b"
  cp -a "$src" "$out/libexec/$b"
  chmod u+w "$out/libexec/$b"
  chmod +x "$out/libexec/$b"
  if patchelf --print-interpreter "$out/libexec/$b" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$out/libexec/$b"
  fi
  patchelf --set-rpath "$rpath" "$out/libexec/$b"
  cat >"$out/bin/$b" <<WRAP
#!/bin/sh
export PATH=/bin
export HOME=/root
export XDG_RUNTIME_DIR=/run/user/0
export XDG_CACHE_HOME=/tmp
export SOLA_NO_SELF_WATCH=1
export SOLA_LOG_DIR=/oath/log
export FONTCONFIG_FILE=/oath/store/pkg/sola/etc/fonts/fonts.conf
export FONTCONFIG_PATH=/oath/store/pkg/sola/etc/fonts
export SOLA_ASSETS_DIR=/oath/store/pkg/sola/share
export XCURSOR_PATH=/oath/store/pkg/sola/share/cursors
export XCURSOR_THEME=McMojave
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export WGPU_BACKEND=gl
export LIBGL_ALWAYS_SOFTWARE=1
# virtio-gpu lists 4K CVT modes; sola-river default is max ≥60Hz.
export SOLA_OUTPUT_PICK=preferred
/bin/mkdir -p /tmp/fontconfig /oath/log
exec /oath/store/pkg/sola/libexec/$b "\$@" >>/oath/log/$b.log 2>&1
WRAP
  chmod +x "$out/bin/$b"
done

for b in "${bins[@]}"; do
  [[ -x $out/libexec/$b ]] || { echo "relocate-sola: missing libexec/$b" >&2; exit 1; }
  [[ -x $out/bin/$b ]] || { echo "relocate-sola: missing bin/$b" >&2; exit 1; }
done
