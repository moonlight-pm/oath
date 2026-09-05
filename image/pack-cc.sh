#!/bin/sh
# Pack pkg:cc from the official Zig linux tarball (static musl ELF).
# Product face is cc / c++ / ar / ranlib / musl-cc / patchelf.
# Default cc target is x86_64-linux-gnu (Sola / Thoxa / rustc host).
# musl-cc is x86_64-linux-musl (Oath guest ELFs).
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

# Wrappers always point at the guest store (same as relocate-git / thoxa).
# glibc ELFs must use pkg:glibc's loader; Oath has no /lib64.
# rustc 1.98 passes -fuse-ld=lld; Zig 0.16 cannot combine that with
# --dynamic-linker. image/zig-gnu-cc.sh drops those flags.
cp "$here/zig-gnu-cc.sh" "$out/libexec/zig-gnu-cc.sh"
chmod 755 "$out/libexec/zig-gnu-cc.sh"
write_cc() {
	cat >"$out/bin/$1" <<EOF
#!/bin/sh
exec $guest/libexec/zig-gnu-cc.sh cc "\$@"
EOF
	chmod 755 "$out/bin/$1"
}
write_cxx() {
	cat >"$out/bin/$1" <<EOF
#!/bin/sh
exec $guest/libexec/zig-gnu-cc.sh c++ "\$@"
EOF
	chmod 755 "$out/bin/$1"
}

write_cc cc
write_cc gcc
write_cxx c++
write_cxx g++

cat >"$out/bin/musl-cc" <<EOF
#!/bin/sh
exec $zigdir/zig cc -target x86_64-linux-musl "\$@"
EOF
chmod 755 "$out/bin/musl-cc"

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

cat >"$out/INDEX.md" <<'EOF'
# pkg:cc

C toolchain for Oath-as-dev-host. Bits are official Zig (static musl)
plus patchelf built with `zig c++`.

`/bin/cc` and `/bin/c++` target **glibc** (`x86_64-linux-gnu`) so Sola
and the rustc host link. `/bin/musl-cc` targets musl for Oath guest
ELFs. `/bin/ar`, `/bin/ranlib`, `/bin/patchelf`.

Not a second libc in PID 1. Zig is the compiler, not a new kind.
EOF

chmod -R u+rwX "$out"
echo "packed cc -> $out"
