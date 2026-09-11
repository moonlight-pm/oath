#!/usr/bin/env bash
# Relocate a glibc foot into $out for pkg:foot. Guest interpreter is pkg:glibc.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_foot=/oath/store/pkg/foot/lib
rpath="$guest_glibc:$guest_foot"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"
FOOT=${FOOT:?}

if [[ -e $out ]]; then
  chmod -R u+w "$out" 2>/dev/null || true
  rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec" "$out/share"

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

foot_bin=
for c in "$FOOT/bin/foot" "$FOOT/bin/.foot-wrapped"; do
  if [[ -e $c ]] && file -b "$c" | grep -q ELF; then
    foot_bin=$c
    break
  fi
done
[[ -n $foot_bin ]] || {
  echo "relocate-foot: no foot ELF under $FOOT/bin" >&2
  ls -l "$FOOT/bin" >&2 || true
  exit 1
}
enqueue "$foot_bin"
for extra in "$FOOT/bin/footclient" "$FOOT/bin/foot-server"; do
  if [[ -e $extra ]] && file -b "$extra" | grep -q ELF; then
    enqueue "$extra"
  fi
done

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

if [[ -n ${LIBUDEV_ZERO:-} && -e $LIBUDEV_ZERO/lib/libudev.so.1 ]]; then
  rm -f "$out/lib"/libudev.so*
  cp -a "$LIBUDEV_ZERO/lib/libudev.so.1" "$out/lib/libudev.so.1"
  chmod u+w "$out/lib/libudev.so.1" 2>/dev/null || true
fi

if [[ -n ${XKB:-} && -d $XKB/share/X11/xkb ]]; then
  mkdir -p "$out/share/X11/xkb"
  cp -aL "$XKB/share/X11/xkb/." "$out/share/X11/xkb/"
fi

# terminfo + desktop. nixpkgs splits terminfo into a sibling output.
if [[ -d $FOOT/share/terminfo ]]; then
  mkdir -p "$out/share/terminfo"
  cp -aL "$FOOT/share/terminfo/." "$out/share/terminfo/"
fi
if [[ -n ${FOOT_TERMINFO:-} && -d $FOOT_TERMINFO/share/terminfo ]]; then
  mkdir -p "$out/share/terminfo"
  cp -aL "$FOOT_TERMINFO/share/terminfo/." "$out/share/terminfo/"
fi
if [[ -d $FOOT/share/applications ]]; then
  mkdir -p "$out/share/applications"
  cp -aL "$FOOT/share/applications/." "$out/share/applications/"
fi

mv "$out/lib/$(basename "$foot_bin")" "$out/libexec/foot"
chmod u+w "$out/libexec/foot"
chmod +x "$out/libexec/foot"
if patchelf --print-interpreter "$out/libexec/foot" >/dev/null 2>&1; then
  patchelf --set-interpreter "$interp_guest" "$out/libexec/foot"
fi
patchelf --set-rpath "$rpath" "$out/libexec/foot"

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

cat >"$out/bin/foot" <<'WRAP'
#!/bin/sh
export PATH=/bin
export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
/bin/mkdir -p "$XDG_RUNTIME_DIR"
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"
export LOCALE_ARCHIVE="${LOCALE_ARCHIVE:-/oath/store/pkg/sola/lib/locale/locale-archive}"
export FONTCONFIG_FILE="${FONTCONFIG_FILE:-/oath/store/pkg/quickshell/etc/fonts/fonts.conf}"
export XKB_CONFIG_ROOT="${XKB_CONFIG_ROOT:-/oath/store/pkg/hyprland/share/X11/xkb}"
export TERMINFO="${TERMINFO:-/oath/store/pkg/foot/share/terminfo}"
[ -f /lib/oath/display-env.sh ] && . /lib/oath/display-env.sh
exec /oath/store/pkg/foot/libexec/foot "$@"
WRAP
chmod +x "$out/bin/foot"
[[ -x $out/libexec/foot ]] || {
  echo "relocate-foot: missing libexec/foot" >&2
  exit 1
}
