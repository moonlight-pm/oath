#!/usr/bin/env bash
# Relocate glibc-linked Sola session ELFs + dlopen deps into $out
# for packing as pkg:sola. Guest rpath includes pkg:glibc and pkg:river.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_river=/oath/store/pkg/river/lib
guest_sola=/oath/store/pkg/sola/lib
guest_cef=/oath/store/pkg/sola/cef/Release
# Session ELFs prefer river's libudev-zero (no udevd). CEF needs the
# versioned systemd libudev we packed into sola/lib.
rpath="$guest_glibc:$guest_river:$guest_sola:$guest_cef"
browser_rpath="$guest_glibc:$guest_sola:$guest_cef:$guest_river"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

# Nix store copies land mode 555; `cp -a` keeps that, and a later
# rebuild cannot replace the tree.
if [[ -e $out ]]; then
  chmod -R u+w "$out" 2>/dev/null || true
  rm -rf "$out"
fi

mkdir -p "$out/lib" "$out/bin" "$out/libexec" \
  "$out/share/fonts" "$out/share/icons" "$out/share/cursors" "$out/etc/fonts"

# Only skip libs that relocate-river actually puts in pkg:glibc.
# libresolv / nss / libutil are not in that closure (River does not
# NEEDED them); tmux does. Those land in pkg:sola/lib.
is_glibc() {
  case "$(basename "$1")" in
    ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
    libgcc_s.so*|libstdc++.so*)
      return 0
      ;;
    *) return 1 ;;
  esac
}

# Shipped next to libcef.so — keep them in cef/Release, not lib/.
is_cef_bundled() {
  case "$(basename "$1")" in
    libcef.so|libEGL.so|libGLESv2.so|libvk_swiftshader.so|libvulkan.so.1|chrome-sandbox)
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

kit_bins=(sola-bus sola-call sola-river sola-shell sola-session sola-terminal sola-browser sola-workspaces solactl)
for b in "${kit_bins[@]}"; do
  src="${SOLA_BINS:?}/$b"
  [[ -f $src ]] || { echo "relocate-sola: missing $src" >&2; exit 1; }
  enqueue "$src"
done

tmux_src=""
if [[ -n ${TMUX_BIN:-} ]]; then
  if [[ -f $TMUX_BIN ]]; then
    tmux_src=$TMUX_BIN
  elif [[ -f $TMUX_BIN/bin/tmux ]]; then
    tmux_src=$TMUX_BIN/bin/tmux
  fi
fi
[[ -n $tmux_src ]] || { echo "relocate-sola: TMUX_BIN missing (need tmux for sola-terminal)" >&2; exit 1; }
enqueue "$tmux_src"

# CEF tree (cache layout Release/ + Resources/). Pack source is
# host `cargo make install-cef`; never commit the binaries.
# Materialize Release (cp -aL): cache uses `../Resources` symlinks and
# busybox tar on metal rewrites those members to `Resources/...`.
cef_src=${CEF_DIR:-}
if [[ -n $cef_src && -f $cef_src/Release/libcef.so ]]; then
  mkdir -p "$out/cef/Release"
  if [[ -d $cef_src/Resources ]]; then
    cp -a "$cef_src/Resources" "$out/cef/Resources"
  fi
  cp -aL "$cef_src/Release/." "$out/cef/Release/"
  chmod -R u+w "$out/cef" 2>/dev/null || true
  enqueue "$cef_src/Release/libcef.so"
elif [[ -n $cef_src && -f $cef_src/libcef.so ]]; then
  mkdir -p "$out/cef/Release"
  cp -aL "$cef_src/." "$out/cef/Release/"
  chmod -R u+w "$out/cef" 2>/dev/null || true
  enqueue "$cef_src/libcef.so"
else
  echo "relocate-sola: CEF_DIR missing libcef.so (set to ~/.cache/sola/cef-<pin>)" >&2
  exit 1
fi

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
  # NSS dlopens sibling modules (softokn, freebl, ckbi) next to libnss3.
  case "$(basename "$f")" in
    libnss3.so*)
      d=$(dirname "$f")
      enqueue_glob "$d"/libsoftokn3.so* "$d"/libfreebl3.so* "$d"/libfreeblpriv3.so* \
        "$d"/libnssckbi.so "$d"/libnssdbm3.so* "$d"/libnsssysinit.so* \
        "$d"/libplc4.so* "$d"/libplds4.so*
      ;;
  esac
done

for f in "${!SEEN[@]}"; do
  name=${SONAME[$f]:-$(basename "$f")}
  if is_glibc "$name"; then
    continue
  fi
  if is_cef_bundled "$name"; then
    continue
  fi
  case "$name" in
    sola-bus|sola-call|sola-river|sola-shell|sola-session|sola-terminal|sola-browser|tmux) continue ;;
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

copy_fonts() {
  local src=$1
  [[ -d $src ]] || return 0
  find "$src" -type f \( -name '*.ttf' -o -name '*.otf' -o -name '*.ttc' \) \
    -exec cp {} "$out/share/fonts/" \;
}

copy_font_globs() {
  local src=$1
  shift
  [[ -d $src ]] || return 0
  local g
  for g in "$@"; do
    find "$src" -type f -name "$g" -exec cp {} "$out/share/fonts/" \;
  done
}

# Kit seed: SF Pro Text (UI) + Iosevka Term Slab (mono). Inter /
# JetBrains Mono stay as fallbacks. SF Pro is operator-licensed — never
# commit the files; pack.rs passes SOLA_SF_FONTS from the host stash.
copy_font_globs "${SOLA_SF_FONTS:-}" 'SF-Pro-Text-*'
# Super-TTC includes Extended/Oblique faces; Regular+Bold is enough for
# the terminal and kit mono role. Do not copy the whole 500M tree.
copy_font_globs "${IOSEVKA_TERM_SLAB:-}/share/fonts" \
  'SGr-IosevkaTermSlab-Regular.ttc' \
  'SGr-IosevkaTermSlab-Bold.ttc'
copy_fonts "${INTER:-}/share/fonts"
copy_fonts "${JETBRAINS_MONO:-}/share/fonts"

# rustls-platform-verifier (vault) and other TLS clients need a CA bundle.
# Chromium still uses NSS (libnssckbi) for page loads.
mkdir -p "$out/etc/ssl/certs"
if [[ -n ${CACERT:-} ]]; then
  for b in "$CACERT/etc/ssl/certs/ca-bundle.crt" \
           "$CACERT/etc/ssl/certs/ca-certificates.crt"; do
    if [[ -f $b ]]; then
      cp "$b" "$out/etc/ssl/certs/ca-certificates.crt"
      break
    fi
  done
fi
[[ -f $out/etc/ssl/certs/ca-certificates.crt ]] || {
  echo "relocate-sola: missing CA bundle (CACERT=cacert)" >&2
  exit 1
}

# winit panics without a Compose file when LANG=C.UTF-8.
mkdir -p "$out/share/X11/locale/en_US.UTF-8"
if [[ -f ${LIBX11:-}/share/X11/locale/en_US.UTF-8/Compose ]]; then
  cp "${LIBX11}/share/X11/locale/en_US.UTF-8/Compose" \
    "$out/share/X11/locale/en_US.UTF-8/Compose"
fi
[[ -f $out/share/X11/locale/en_US.UTF-8/Compose ]] || {
  echo "relocate-sola: missing X11 Compose (LIBX11)" >&2
  exit 1
}

# glibc tmux refuses to start without a UTF-8 locale. C.UTF-8 archive.
if [[ -n ${LOCALES:-} ]]; then
  arch=""
  if [[ -f $LOCALES/lib/locale/locale-archive ]]; then
    arch=$LOCALES/lib/locale/locale-archive
  else
    arch=$(find "$LOCALES" -name locale-archive -type f 2>/dev/null | head -n1 || true)
  fi
  if [[ -n $arch ]]; then
    mkdir -p "$out/lib/locale"
    cp "$arch" "$out/lib/locale/locale-archive"
    chmod u+w "$out/lib/locale/locale-archive" 2>/dev/null || true
  fi
fi
[[ -f $out/lib/locale/locale-archive ]] || {
  echo "relocate-sola: missing locale-archive (LOCALES=C.UTF-8)" >&2
  exit 1
}

# tmux / inner shells need a few terminfo entries, not the whole tree.
if [[ -n ${NCURSES:-} ]]; then
  term_root=""
  for src in "$NCURSES/share/terminfo" "$NCURSES/lib/terminfo"; do
    if [[ -d $src ]]; then
      term_root=$src
      break
    fi
  done
  if [[ -n $term_root ]]; then
    mkdir -p "$out/share/terminfo"
    for t in xterm xterm-256color tmux tmux-256color screen screen-256color; do
      letter=${t:0:1}
      if [[ -e $term_root/$letter/$t ]]; then
        mkdir -p "$out/share/terminfo/$letter"
        chmod u+w "$out/share/terminfo" "$out/share/terminfo/$letter"
        cp "$term_root/$letter/$t" "$out/share/terminfo/$letter/$t"
        chmod u+w "$out/share/terminfo/$letter/$t" 2>/dev/null || true
      fi
    done
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

patchelf_guest() {
  local f=$1
  local rp=${2:-$rpath}
  file -b "$f" | grep -q ELF || return 0
  chmod u+w "$f" || true
  if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$f" || true
  fi
  patchelf --set-rpath "$rp" "$f" 2>/dev/null || true
}

find "$out/lib" -type f 2>/dev/null | while read -r f; do
  patchelf_guest "$f" "$rpath"
done
# CEF's own SOs must see $ORIGIN (siblings) plus sola/lib before river
# (systemd libudev vs libudev-zero).
if [[ -d $out/cef ]]; then
  find "$out/cef" -type f 2>/dev/null | while read -r f; do
    patchelf_guest "$f" "\$ORIGIN:$browser_rpath"
  done
fi

patchelf_libexec() {
  local dest=$1
  chmod u+w "$dest"
  chmod +x "$dest"
  if patchelf --print-interpreter "$dest" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$dest"
  fi
  patchelf --set-rpath "$rpath" "$dest"
}

guest_env='export PATH=/bin
export HOME="${HOME:-/home}"
export SHELL=/bin/sh
export LANG=C.UTF-8
export LC_ALL=C.UTF-8
export LOCALE_ARCHIVE=/oath/store/pkg/sola/lib/locale/locale-archive
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_CACHE_HOME=/tmp
export SOLA_NO_SELF_WATCH=1
export SOLA_LOG_DIR=/oath/log
export FONTCONFIG_FILE=/oath/store/pkg/sola/etc/fonts/fonts.conf
export FONTCONFIG_PATH=/oath/store/pkg/sola/etc/fonts
export SOLA_ASSETS_DIR=/oath/store/pkg/sola/share
export SOLA_CEF_DIR=/oath/store/pkg/sola/cef
export SOLA_BROWSER=/bin/sola-browser
export SSL_CERT_FILE=/oath/store/pkg/sola/etc/ssl/certs/ca-certificates.crt
export SSL_CERT_DIR=/oath/store/pkg/sola/etc/ssl/certs
export CURL_CA_BUNDLE=/oath/store/pkg/sola/etc/ssl/certs/ca-certificates.crt
export XCOMPOSEFILE=/oath/store/pkg/sola/share/X11/locale/en_US.UTF-8/Compose
export XKB_CONFIG_ROOT=/oath/store/pkg/river/share/X11/xkb
export XCURSOR_PATH=/oath/store/pkg/sola/share/cursors
export XCURSOR_THEME=McMojave
export TERMINFO=/oath/store/pkg/sola/share/terminfo
export TERM=xterm-256color
export LIBGL_DRIVERS_PATH=/oath/store/pkg/river/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/river/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/river/share/glvnd/egl_vendor.d/50_mesa.json
export WGPU_BACKEND=gl
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
# virtio-gpu lists 4K CVT modes; sola-river default is max ≥60Hz.
export SOLA_OUTPUT_PICK=preferred'

for b in "${kit_bins[@]}"; do
  src="$SOLA_BINS/$b"
  cp -a "$src" "$out/libexec/$b"
  if [[ $b == sola-browser ]]; then
    chmod u+w "$out/libexec/$b"
    chmod +x "$out/libexec/$b"
    if patchelf --print-interpreter "$out/libexec/$b" >/dev/null 2>&1; then
      patchelf --set-interpreter "$interp_guest" "$out/libexec/$b"
    fi
    patchelf --set-rpath "$browser_rpath" "$out/libexec/$b"
  else
    patchelf_libexec "$out/libexec/$b"
  fi
  cat >"$out/bin/$b" <<WRAP
#!/bin/sh
$guest_env
/bin/mkdir -p /tmp/fontconfig /oath/log "\$HOME/.local/share" "\$HOME/.config"
exec /oath/store/pkg/sola/libexec/$b "\$@" >>/oath/log/$b.log 2>&1
WRAP
  chmod +x "$out/bin/$b"
done

# tmux is a PTY child — must not steal stdout into a log file.
cp -a "$tmux_src" "$out/libexec/tmux"
patchelf_libexec "$out/libexec/tmux"
cat >"$out/bin/tmux" <<WRAP
#!/bin/sh
$guest_env
exec /oath/store/pkg/sola/libexec/tmux "\$@"
WRAP
chmod +x "$out/bin/tmux"

# solactl is a CLI — stdout is the product, not a log.
cat >"$out/bin/solactl" <<WRAP
#!/bin/sh
$guest_env
exec /oath/store/pkg/sola/libexec/solactl "\$@"
WRAP
chmod +x "$out/bin/solactl"

for b in "${kit_bins[@]}" tmux; do
  [[ -x $out/libexec/$b ]] || { echo "relocate-sola: missing libexec/$b" >&2; exit 1; }
  [[ -x $out/bin/$b ]] || { echo "relocate-sola: missing bin/$b" >&2; exit 1; }
done
[[ -d $out/share/terminfo ]] || { echo "relocate-sola: missing share/terminfo" >&2; exit 1; }
ls "$out"/share/fonts/SF-Pro-Text-* >/dev/null 2>&1 || {
  echo "relocate-sola: missing SF Pro Text (set SOLA_SF_FONTS to the sola-sf dir)" >&2
  exit 1
}
ls "$out"/share/fonts/*IosevkaTermSlab* >/dev/null 2>&1 || {
  echo "relocate-sola: missing Iosevka Term Slab" >&2
  exit 1
}
[[ -f $out/cef/Release/libcef.so ]] || {
  echo "relocate-sola: missing cef/Release/libcef.so" >&2
  exit 1
}
[[ -f $out/etc/ssl/certs/ca-certificates.crt ]] || {
  echo "relocate-sola: missing etc/ssl/certs/ca-certificates.crt" >&2
  exit 1
}
