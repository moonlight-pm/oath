#!/usr/bin/env bash
# Relocate a glibc Thoxa (compiler + session REPL + std + libtcc1) into $out
# for pkg:thoxa. Guest rpath includes pkg:glibc.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_thoxa=/oath/store/pkg/thoxa/lib
rpath="$guest_glibc:$guest_thoxa"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"
THOXA_SRC=${THOXA_SRC:?}
THOXA_BIN=${THOXA_BIN:?}

if [[ -e $out ]]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec" \
	"$out/std" "$out/runtime/include" "$out/runtime/src" \
	"$out/target/c" "$out/lib/tcc" "$out/include"

is_glibc() {
	case "$(basename "$1")" in
	ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
	libgcc_s.so*|libstdc++.so*)
		return 0
		;;
	*) return 1 ;;
	esac
}

cp -a "$THOXA_BIN" "$out/libexec/thoxa"
chmod u+w "$out/libexec/thoxa"
chmod +x "$out/libexec/thoxa"
if patchelf --print-interpreter "$out/libexec/thoxa" >/dev/null 2>&1; then
	patchelf --set-interpreter "$interp_guest" "$out/libexec/thoxa"
fi
patchelf --set-rpath "$rpath" "$out/libexec/thoxa"

# Shared libs the ELF still NEEDs besides glibc (usually none besides libm/gcc).
while IFS= read -r line; do
	lib=${line#*=> }
	lib=${lib%% *}
	[[ $lib == /* ]] || continue
	base=$(basename "$lib")
	if is_glibc "$lib"; then
		continue
	fi
	if [[ $base == libm.so* ]]; then
		continue
	fi
	cp -L "$lib" "$out/lib/$base" 2>/dev/null || true
done < <(ldd "$THOXA_BIN" 2>/dev/null || true)

cp -a "$THOXA_SRC/std/." "$out/std/"
cp -a "$THOXA_SRC/runtime/include/." "$out/runtime/include/"
# Prebuilt runtime archive so session steps do not need cc on the guest.
if [[ -f $THOXA_SRC/target/c/libthoxa_rt.a ]]; then
	cp -a "$THOXA_SRC/target/c/libthoxa_rt.a" "$out/target/c/libthoxa_rt.a"
fi
TCC="$THOXA_SRC/target/c/tinycc/lib/tcc"
if [[ -d $TCC ]]; then
	cp -a "$TCC/." "$out/lib/tcc/"
fi
# TinyCC -lc looks for libc.so; pkg:glibc ships libc.so.6. A regular
# GNU ld script (not an absolute symlink) survives busybox tar.
cat >"$out/lib/tcc/libc.so" <<'LIBC'
GROUP ( /oath/store/pkg/glibc/lib/libc.so.6 )
LIBC

# Host C headers so libtcc can compile session steps (stdint.h, unistd.h, …).
# Walk gcc's quote/include list; copy files, not nix store prefixes.
inc_idx=0
while IFS= read -r dir; do
	dir=$(echo "$dir" | sed 's/^[[:space:]]*//')
	[[ -d $dir ]] || continue
	dest="$out/include/$inc_idx"
	mkdir -p "$dest"
	cp -aL "$dir/." "$dest/" 2>/dev/null || true
	inc_idx=$((inc_idx + 1))
done < <(
	echo | gcc -E -Wp,-v -xc - -o /dev/null 2>&1 |
		awk '/^#include <...> search starts here:/{p=1;next} /^End of search list/{p=0} p && $1 ~ /^\//{print $1}'
)

# Colon list of packed include dirs for THOXA_C_INCLUDE.
c_include=""
for d in "$out"/include/*; do
	[[ -d $d ]] || continue
	name=/oath/store/pkg/thoxa/include/$(basename "$d")
	if [[ -z $c_include ]]; then
		c_include=$name
	else
		c_include="$c_include:$name"
	fi
done
c_include="$c_include:/oath/store/pkg/thoxa/lib/tcc/include"

cat >"$out/bin/thoxa" <<WRAP
#!/bin/sh
export PATH="\${PATH:-/bin}"
export HOME="\${HOME:-/home}"
export THOXA_ROOT=/oath/store/pkg/thoxa
export THOXA_STD=/oath/store/pkg/thoxa/std
export THOXA_TCCDIR=/oath/store/pkg/thoxa/lib/tcc
export THOXA_C_INCLUDE=$c_include
export THOXA_TCC_LIBPATHS=/oath/store/pkg/glibc/lib:/oath/store/pkg/thoxa/lib/tcc
export THOXA_TOOLCHAIN_PATH=/bin
exec /oath/store/pkg/thoxa/libexec/thoxa "\$@"
WRAP
chmod 755 "$out/bin/thoxa"
chmod -R u+rwX "$out"
