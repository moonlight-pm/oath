#!/usr/bin/env bash
# Relocate Quickshell + Qt6 into $out for pkg:quickshell.
# Guest interpreter is pkg:glibc. GL/EGL comes from pkg:hyprland's Mesa.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_qs=/oath/store/pkg/quickshell/lib
guest_hypr=/oath/store/pkg/hyprland/lib
rpath="$guest_glibc:$guest_qs:$guest_hypr"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

mkdir -p "$out/lib" "$out/bin" "$out/libexec" \
  "$out/lib/qt-6/plugins" "$out/lib/qt-6/qml"

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

qs_bin=
# nixpkgs wrapProgram emits a 16k makeCWrapper that execv's .quickshell-wrapped.
# Prefer the wrapped ELF. `strings` is optional (nix sandbox may omit it).
for c in "$QUICKSHELL/bin/.quickshell-wrapped" "$QUICKSHELL/bin/quickshell" "$QUICKSHELL/bin/qs"; do
  if [[ -e $c ]] && file -b "$c" | grep -q ELF; then
    if command -v strings >/dev/null 2>&1 && strings "$c" | grep -q makeCWrapper; then
      continue
    fi
    enqueue "$c"
    [[ -z $qs_bin ]] && qs_bin=$c
  fi
done
[[ -n $qs_bin ]] || {
  echo "relocate-quickshell: no quickshell ELF under $QUICKSHELL/bin" >&2
  ls -l "$QUICKSHELL/bin" >&2 || true
  exit 1
}

copy_tree_enqueue() {
  local src=$1 dest=$2
  [[ -d $src ]] || return 0
  mkdir -p "$dest"
  cp -a "$src/." "$dest/"
  chmod -R u+rwX "$dest"
  while IFS= read -r f; do
    file -b "$f" | grep -q ELF || continue
    enqueue "$f"
  done < <(find "$dest" -type f)
}

copy_tree_enqueue "${QUICKSHELL}/lib/qt-6/qml" "$out/lib/qt-6/qml"
copy_tree_enqueue "${QTDECLARATIVE}/lib/qt-6/qml" "$out/lib/qt-6/qml"
copy_tree_enqueue "${QTSVG}/lib/qt-6/qml" "$out/lib/qt-6/qml"
copy_tree_enqueue "${QTWAYLAND}/lib/qt-6/qml" "$out/lib/qt-6/qml"

for plug in platforms wayland-decoration-client wayland-graphics-integration-client \
  wayland-graphics-integration-server wayland-shell-integration \
  egldeviceintegrations imageformats platforminputcontexts platformthemes \
  generic tls iconengines; do
  copy_tree_enqueue "${QTBASE}/lib/qt-6/plugins/${plug}" "$out/lib/qt-6/plugins/${plug}"
  copy_tree_enqueue "${QTWAYLAND}/lib/qt-6/plugins/${plug}" "$out/lib/qt-6/plugins/${plug}"
  copy_tree_enqueue "${QTSVG}/lib/qt-6/plugins/${plug}" "$out/lib/qt-6/plugins/${plug}"
done

loader=""
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
  case "$f" in
    "$out"/*) continue ;;
  esac
  d="$out/lib/$name"
  if [[ -e $d ]]; then
    continue
  fi
  mkdir -p "$(dirname "$d")"
  cp -a "$f" "$d"
  chmod u+w "$d" 2>/dev/null || true
done

if [[ -n ${LIBUDEV_ZERO:-} && -e $LIBUDEV_ZERO/lib/libudev.so.1 ]]; then
  rm -f "$out/lib"/libudev.so*
  cp -a "$LIBUDEV_ZERO/lib/libudev.so.1" "$out/lib/libudev.so.1"
  chmod u+w "$out/lib/libudev.so.1" 2>/dev/null || true
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

qs_name=$(basename "$qs_bin")
if [[ -e $out/lib/$qs_name ]]; then
  mv "$out/lib/$qs_name" "$out/libexec/quickshell"
else
  cp -a "$qs_bin" "$out/libexec/quickshell"
fi
chmod +x "$out/libexec/quickshell"
if patchelf --print-interpreter "$out/libexec/quickshell" >/dev/null 2>&1; then
  patchelf --set-interpreter "$interp_guest" "$out/libexec/quickshell"
fi
patchelf --set-rpath "$rpath" "$out/libexec/quickshell"

if [[ -e $out/lib/qs ]]; then
  mv "$out/lib/qs" "$out/libexec/qs"
  chmod +x "$out/libexec/qs"
  if patchelf --print-interpreter "$out/libexec/qs" >/dev/null 2>&1; then
    patchelf --set-interpreter "$interp_guest" "$out/libexec/qs"
  fi
  patchelf --set-rpath "$rpath" "$out/libexec/qs"
  ln -sfn /oath/store/pkg/quickshell/libexec/qs "$out/bin/qs"
fi

# Qt Wayland bakes a nixpkgs xkeyboard-config path. Copy xkb into this pack
# and NUL-pad-replace the bake so the guest does not need /nix/store.
xkb_guest=/oath/store/pkg/hyprland/share/X11/xkb
if [[ -n ${XKB:-} && -d $XKB/share/X11/xkb ]]; then
  mkdir -p "$out/share/X11/xkb"
  cp -aL "$XKB/share/X11/xkb/." "$out/share/X11/xkb/"
  xkb_guest=/oath/store/pkg/quickshell/share/X11/xkb
elif [[ -d /oath/store/pkg/hyprland/share/X11/xkb ]]; then
  mkdir -p "$out/share/X11/xkb"
  cp -aL /oath/store/pkg/hyprland/share/X11/xkb/. "$out/share/X11/xkb/" 2>/dev/null || true
fi
if command -v perl >/dev/null 2>&1; then
  find "$out" -type f | while read -r f; do
    file -b "$f" | grep -q ELF || continue
    XKB_GUEST="$xkb_guest" perl -0777 -i -e '
      binmode ARGV; binmode STDOUT;
      my $n = $ENV{XKB_GUEST} // "";
      length $n or exit 0;
      local $/;
      my $s = <>;
      defined $s or exit 0;
      $s =~ s{/nix/store/[0-9a-z]{32}-xkeyboard-config-[^\0/]+/(?:share|etc)/X11/xkb}{
        my $old = $&;
        my $pad = length($old) - length($n);
        $pad >= 0 ? $n . ("\0" x $pad) : $old
      }ge;
      print $s;
    ' "$f" || true
  done
fi

mkdir -p "$out/etc/fonts"
cat >"$out/etc/fonts/fonts.conf" <<'FC'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/oath/store/pkg/sola/share/fonts</dir>
  <dir>/oath/store/pkg/omarchy/default/fonts</dir>
  <dir>/oath/store/pkg/omarchy/default/fontconfig</dir>
</fontconfig>
FC

cat >"$out/bin/quickshell" <<'WRAP'
#!/bin/sh
export PATH="/oath/store/pkg/omarchy/bin:/bin"
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
/bin/mkdir -p "$XDG_RUNTIME_DIR"
/bin/chmod 700 "$XDG_RUNTIME_DIR"
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"
export LOCALE_ARCHIVE="${LOCALE_ARCHIVE:-/oath/store/pkg/sola/lib/locale/locale-archive}"
export QT_QPA_PLATFORM=wayland
export QT_PLUGIN_PATH=/oath/store/pkg/quickshell/lib/qt-6/plugins
export QML2_IMPORT_PATH=/oath/store/pkg/quickshell/lib/qt-6/qml
export QML_IMPORT_PATH="$QML2_IMPORT_PATH"
export NIXPKGS_QT6_QML_IMPORT_PATH="$QML2_IMPORT_PATH"
export QT_QPA_PLATFORM_PLUGIN_PATH=/oath/store/pkg/quickshell/lib/qt-6/plugins/platforms
export LIBGL_DRIVERS_PATH=/oath/store/pkg/hyprland/lib/dri
export GBM_BACKENDS_PATH=/oath/store/pkg/hyprland/lib/gbm
export __EGL_VENDOR_LIBRARY_FILENAMES=/oath/store/pkg/hyprland/share/glvnd/egl_vendor.d/50_mesa.json
if [ -d /oath/store/pkg/quickshell/share/X11/xkb ]; then
	export XKB_CONFIG_ROOT=/oath/store/pkg/quickshell/share/X11/xkb
else
	export XKB_CONFIG_ROOT=/oath/store/pkg/hyprland/share/X11/xkb
fi
export LIBINPUT_QUIRKS_DIR=/oath/store/pkg/hyprland/share/libinput
export OMARCHY_PATH="${OMARCHY_PATH:-/oath/store/pkg/omarchy}"
export FONTCONFIG_FILE=/oath/store/pkg/quickshell/etc/fonts/fonts.conf
export QS_DISABLE_FILE_WATCHER=1
export QS_NO_RELOAD_POPUP=1
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
# Wait for Hyprland's Wayland socket (PID 1 does not inherit it).
i=0
while [ "$i" -lt 50 ]; do
	for s in "$XDG_RUNTIME_DIR"/wayland-*; do
		[ -S "$s" ] || continue
		case "$s" in
		*.lock) continue ;;
		esac
		export WAYLAND_DISPLAY=$(basename "$s")
		break
	done
	[ -n "${WAYLAND_DISPLAY:-}" ] && break
	i=$((i + 1))
	/bin/sleep 0.2
done
[ -n "${WAYLAND_DISPLAY:-}" ] || {
	echo "quickshell: no wayland socket in $XDG_RUNTIME_DIR" >&2
	exit 1
}
# Hyprland crash-loops leave stale $XDG_RUNTIME_DIR/hypr/* dirs. Only the
# live instance still has .socket.sock.
if [ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
	for d in "$XDG_RUNTIME_DIR"/hypr/*; do
		[ -S "$d/.socket.sock" ] || continue
		export HYPRLAND_INSTANCE_SIGNATURE=$(basename "$d")
		break
	done
fi
if [ -d "$OMARCHY_PATH/shell" ]; then
	set -- -n -p "$OMARCHY_PATH/shell" "$@"
fi
exec /oath/store/pkg/quickshell/libexec/quickshell "$@" >>/oath/log/omarchy-shell.log 2>&1
WRAP
chmod +x "$out/bin/quickshell"
[[ -x $out/libexec/quickshell ]] || {
  echo "relocate-quickshell: missing libexec/quickshell" >&2
  exit 1
}
