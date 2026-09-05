#!/bin/sh
# Pack pkg:cmake from Kitware's linux-x86_64 tarball plus ninja.
set -eu

out=${1:?out}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
fetch=$here/fetch-url.sh

CMAKE_VER=4.3.5
CMAKE_URL=https://github.com/Kitware/CMake/releases/download/v${CMAKE_VER}/cmake-${CMAKE_VER}-linux-x86_64.tar.gz
NINJA_VER=1.13.2
NINJA_URL=https://github.com/ninja-build/ninja/releases/download/v${NINJA_VER}/ninja-linux.zip

guest=/oath/store/pkg/cmake
guest_glibc=/oath/store/pkg/glibc/lib
interp=$guest_glibc/ld-linux-x86-64.so.2
cache=${OATH_FETCH:-${TMPDIR:-/tmp}/oath-fetch}
patchelf=${PATCHELF:-}

if [ -e "$out" ]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/libexec" "$out/share"

ctar=$cache/cmake-${CMAKE_VER}-linux-x86_64.tar.gz
sh "$fetch" "$CMAKE_URL" "$ctar"
tar -xzf "$ctar" -C "$cache"
csrc=$cache/cmake-${CMAKE_VER}-linux-x86_64
cp -a "$csrc/bin/." "$out/libexec/"
if [ -d "$csrc/share" ]; then
	cp -a "$csrc/share/." "$out/share/"
fi

nzip=$cache/ninja-linux-${NINJA_VER}.zip
sh "$fetch" "$NINJA_URL" "$nzip"
mkdir -p "$cache/ninja-${NINJA_VER}"
unzip -o -d "$cache/ninja-${NINJA_VER}" "$nzip"
cp -a "$cache/ninja-${NINJA_VER}/ninja" "$out/libexec/ninja"
chmod 755 "$out/libexec/ninja"

relocate_elf() {
	f=$1
	[ -f "$f" ] || return 0
	dd if="$f" bs=4 count=1 2>/dev/null | od -An -tx1 | grep -q '7f 45 4c 46' || return 0
	chmod u+w "$f" || true
	if [ -n "$patchelf" ] && [ -x "$patchelf" ]; then
		if "$patchelf" --print-interpreter "$f" >/dev/null 2>&1; then
			"$patchelf" --set-interpreter "$interp" "$f" || true
		fi
		"$patchelf" --set-rpath "$guest_glibc" "$f" 2>/dev/null || true
	fi
}

for f in "$out/libexec"/*; do
	relocate_elf "$f"
done

wrap() {
	name=$1
	[ -f "$out/libexec/$name" ] || return 0
	cat >"$out/bin/$name" <<EOF
#!/bin/sh
export LD_LIBRARY_PATH="$guest_glibc\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export CMAKE_ROOT="$guest/share/cmake-4.3"
export CMAKE_PREFIX_PATH="$guest"
if [ -x $interp ]; then
	exec $interp --library-path "\$LD_LIBRARY_PATH" $guest/libexec/$name "\$@"
fi
exec $guest/libexec/$name "\$@"
EOF
	chmod 755 "$out/bin/$name"
}

wrap cmake
wrap ctest
wrap cpack
wrap ccmake
wrap ninja

printf '%s\n' "cmake $CMAKE_VER" "ninja $NINJA_VER" >"$out/REV"

cat >"$out/INDEX.md" <<'EOF'
# pkg:cmake

Kitware official linux-x86_64 cmake plus ninja (the generator).
glibc payload, `pkg:glibc` loader. Removable.

`CMAKE_GENERATOR=Ninja` is what aws-lc-sys wants when `make` is absent.
EOF

chmod -R u+rwX "$out"
echo "packed cmake -> $out"
