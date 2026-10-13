#!/bin/sh
# Pack pkg:cc from the official Zig linux tarball (static musl ELF).
# Product face is cc / c++ / ar / ranlib / musl-cc / patchelf.
# Every compiler wrapper targets x86_64-linux-musl. Oath has no glibc.
set -eu

out=${1:?out}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
fetch=$here/fetch-url.sh

ZIG_VER=0.16.0
ZIG_URL=https://ziglang.org/download/${ZIG_VER}/zig-x86_64-linux-${ZIG_VER}.tar.xz
ZIG_SHA=70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00

PATCHELF_VER=0.18.0
PATCHELF_URL=https://github.com/NixOS/patchelf/releases/download/${PATCHELF_VER}/patchelf-${PATCHELF_VER}.tar.gz

guest=/oath/store/pkg/cc
zigdir=$guest/libexec/zig
cache=${OATH_FETCH:-${TMPDIR:-/tmp}/oath-fetch}

if [ -e "$out" ]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/libexec/zig" "$out/lib"

tarball=$cache/zig-x86_64-linux-${ZIG_VER}.tar.xz
sh "$fetch" "$ZIG_URL" "$tarball" "$ZIG_SHA"
tar -xJf "$tarball" -C "$cache"
src=$cache/zig-x86_64-linux-${ZIG_VER}
cp -a "$src/zig" "$out/libexec/zig/zig"
chmod 755 "$out/libexec/zig/zig"
cp -a "$src/lib" "$out/libexec/zig/lib"
if [ -f "$src/LICENSE" ]; then
	cp -a "$src/LICENSE" "$out/libexec/zig/LICENSE"
fi

# Wrappers always point at the guest store.
write_zig() {
	cat >"$out/bin/$1" <<EOF
#!/bin/sh
exec $zigdir/zig $2 -target x86_64-linux-musl "\$@"
EOF
	chmod 755 "$out/bin/$1"
}

write_zig cc cc
write_zig gcc cc
write_zig musl-cc cc
write_zig c++ c++
write_zig g++ c++

cat >"$out/bin/ar" <<EOF
#!/bin/sh
exec $zigdir/zig ar "\$@"
EOF
chmod 755 "$out/bin/ar"

cat >"$out/bin/ranlib" <<EOF
#!/bin/sh
exec $zigdir/zig ranlib "\$@"
EOF
chmod 755 "$out/bin/ranlib"

# patchelf: compile from source with zig c++ (no make/configure).
ptar=$cache/patchelf-${PATCHELF_VER}.tar.gz
sh "$fetch" "$PATCHELF_URL" "$ptar"
tar -xzf "$ptar" -C "$cache"
psrc=$cache/patchelf-${PATCHELF_VER}
# Single translation unit in recent patchelf.
if [ -f "$psrc/src/patchelf.cc" ]; then
	"$out/libexec/zig/zig" c++ -O2 -std=c++17 -Wno-nullability-completeness \
		-D_FILE_OFFSET_BITS=64 \
		-o "$out/libexec/patchelf" "$psrc/src/patchelf.cc"
elif [ -f "$psrc/src/patchelf.cpp" ]; then
	"$out/libexec/zig/zig" c++ -O2 -std=c++17 -Wno-nullability-completeness \
		-D_FILE_OFFSET_BITS=64 \
		-o "$out/libexec/patchelf" "$psrc/src/patchelf.cpp"
else
	echo "patchelf source not found under $psrc/src" >&2
	exit 1
fi
chmod 755 "$out/libexec/patchelf"
cat >"$out/bin/patchelf" <<EOF
#!/bin/sh
exec $guest/libexec/patchelf "\$@"
EOF
chmod 755 "$out/bin/patchelf"

printf '%s\n' "zig $ZIG_VER" "patchelf $PATCHELF_VER" >"$out/REV"
sh "$here/write-components.sh" "$out" \
	zig "$ZIG_VER" "pkg:generic/zig@$ZIG_VER" \
	patchelf "$PATCHELF_VER" "pkg:github/NixOS/patchelf@$PATCHELF_VER"

cat >"$out/INDEX.md" <<'EOF'
# pkg:cc

C toolchain for Oath-as-dev-host. Bits are official Zig (static musl)
plus patchelf built with `zig c++`.

`/bin/cc`, `/bin/c++` and `/bin/musl-cc` all target musl
(`x86_64-linux-musl`). `/bin/ar`, `/bin/ranlib`, `/bin/patchelf`.

Zig is the compiler, not a new kind.
EOF

chmod -R u+rwX "$out"
echo "packed cc -> $out"
