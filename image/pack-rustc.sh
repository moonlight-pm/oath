#!/bin/sh
# Pack pkg:rustc from the official standalone GNU toolchain plus musl std.
# Host rustc is glibc (wrapped / patchelf'd onto pkg:glibc). rust-std musl
# is the compile target for Oath guest ELFs.
set -eu

out=${1:?out}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
fetch=$here/fetch-url.sh

RUST_VER=1.98.1
RUST_DATE=2026-09-03
# gzip: busybox tar -xJf fails on the official xz (sha256 matches, extract short-reads).
RUST_URL=https://static.rust-lang.org/dist/${RUST_DATE}/rust-${RUST_VER}-x86_64-unknown-linux-gnu.tar.gz
RUST_SHA=24ba1338a2d35c5a3247936546429e163fa674d726102af18bdf624582c57aea
MUSL_URL=https://static.rust-lang.org/dist/${RUST_DATE}/rust-std-${RUST_VER}-x86_64-unknown-linux-musl.tar.gz
MUSL_SHA=ac111ad77967e98d0362e49a2c19121f3f54e4e68c247b90995e665687ebede8

guest=/oath/store/pkg/rustc
guest_glibc=/oath/store/pkg/glibc/lib
interp=$guest_glibc/ld-linux-x86-64.so.2
cache=${OATH_FETCH:-${TMPDIR:-/tmp}/oath-fetch}
patchelf=${PATCHELF:-}

if [ -e "$out" ]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec"

tarball=$cache/rust-${RUST_VER}-x86_64-unknown-linux-gnu.tar.gz
sh "$fetch" "$RUST_URL" "$tarball" "$RUST_SHA"
echo "extract rust $RUST_VER"
tar -xzf "$tarball" -C "$cache" || {
	echo "tar extract failed for $tarball" >&2
	exit 1
}
rsrc=$cache/rust-${RUST_VER}-x86_64-unknown-linux-gnu
if [ ! -x "$rsrc/install.sh" ] && [ ! -d "$rsrc/rustc" ]; then
	echo "rust extract missing $rsrc" >&2
	exit 1
fi

# Combined installer layout: rustc/, cargo/, rust-std-*/ under the tarball.
prefix=$cache/rust-prefix-$$
rm -rf "$prefix"
mkdir -p "$prefix"
# install.sh is POSIX; --disable-ldconfig avoids a host ldconfig.
if [ -x "$rsrc/install.sh" ]; then
	sh "$rsrc/install.sh" --prefix="$prefix" --disable-ldconfig >/dev/null
else
	echo "rust install.sh missing" >&2
	exit 1
fi

mtar=$cache/rust-std-${RUST_VER}-x86_64-unknown-linux-musl.tar.gz
sh "$fetch" "$MUSL_URL" "$mtar" "$MUSL_SHA"
echo "extract rust-std musl $RUST_VER"
tar -xzf "$mtar" -C "$cache" || {
	echo "tar extract failed for $mtar" >&2
	exit 1
}
msrc=$cache/rust-std-${RUST_VER}-x86_64-unknown-linux-musl
if [ -x "$msrc/install.sh" ]; then
	sh "$msrc/install.sh" --prefix="$prefix" --disable-ldconfig >/dev/null
fi

# Copy the prefix. Bins go to libexec (real ELFs); wrappers in bin/.
cp -a "$prefix/lib/." "$out/lib/"
# rustc DT_NEEDED libz; pkg:glibc does not ship it. Copy from git if present.
if [ -f /oath/store/pkg/git/lib/libz.so.1 ]; then
	cp -a /oath/store/pkg/git/lib/libz.so* "$out/lib/" 2>/dev/null || true
fi
mkdir -p "$out/libexec"
for b in rustc cargo rustdoc rustfmt clippy-driver; do
	if [ -f "$prefix/bin/$b" ]; then
		cp -a "$prefix/bin/$b" "$out/libexec/$b"
		chmod 755 "$out/libexec/$b"
	fi
done
# rust-lld / gcc-ld live next to rustc in lib/rustlib/.../bin — already in lib/.

relocate_elf() {
	f=$1
	[ -f "$f" ] || return 0
	# ELF?
	dd if="$f" bs=4 count=1 2>/dev/null | od -An -tx1 | grep -q '7f 45 4c 46' || return 0
	chmod u+w "$f" || true
	if [ -n "$patchelf" ] && [ -x "$patchelf" ]; then
		if "$patchelf" --print-interpreter "$f" >/dev/null 2>&1; then
			"$patchelf" --set-interpreter "$interp" "$f" || true
		fi
		"$patchelf" --set-rpath "$guest_glibc:$guest/lib" "$f" 2>/dev/null || true
	fi
}

# Walk copied ELFs.
find "$out/lib" "$out/libexec" -type f 2>/dev/null | while read -r f; do
	relocate_elf "$f"
done

# Wrappers: always pass --sysroot so ld.so-exec still finds rustlib.
# After patchelf, /proc/self/exe is the libexec ELF; --sysroot is still
# correct and lets us skip depending on argv0.
write_wrap() {
	name=$1
	extra=$2
	[ -f "$out/libexec/$name" ] || return 0
	cat >"$out/bin/$name" <<EOF
#!/bin/sh
export LD_LIBRARY_PATH="$guest/lib:$guest_glibc:/oath/store/pkg/git/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export CC="\${CC:-/bin/cc}"
export CXX="\${CXX:-/bin/c++}"
export AR="\${AR:-/bin/ar}"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="\${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-/bin/musl-cc}"
if [ -x $interp ]; then
	exec $interp --library-path "\$LD_LIBRARY_PATH" $guest/libexec/$name $extra "\$@"
fi
exec $guest/libexec/$name $extra "\$@"
EOF
	chmod 755 "$out/bin/$name"
}

write_cargo() {
	[ -f "$out/libexec/cargo" ] || return 0
	cat >"$out/bin/cargo" <<EOF
#!/bin/sh
export LD_LIBRARY_PATH="$guest/lib:$guest_glibc:/oath/store/pkg/git/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export CC="\${CC:-/bin/cc}"
export CXX="\${CXX:-/bin/c++}"
export AR="\${AR:-/bin/ar}"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="\${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-/bin/musl-cc}"
export RUSTC="\${RUSTC:-/bin/rustc}"
if [ -x $interp ]; then
	exec $interp --library-path "\$LD_LIBRARY_PATH" $guest/libexec/cargo "\$@"
fi
exec $guest/libexec/cargo "\$@"
EOF
	chmod 755 "$out/bin/cargo"
}

write_wrap rustc "--sysroot $guest"
write_wrap rustdoc "--sysroot $guest"
write_wrap rustfmt ""
write_wrap clippy-driver ""
write_cargo

# rustc --sysroot on rustdoc/clippy too. cargo must NOT get --sysroot.

printf '%s\n' "rustc $RUST_VER gnu host" "rust-std $RUST_VER musl" >"$out/REV"

cat >"$out/INDEX.md" <<'EOF'
# pkg:rustc

Official Rust standalone toolchain. Host is `x86_64-unknown-linux-gnu`
(glibc, `pkg:glibc` loader). `rust-std` for `x86_64-unknown-linux-musl`
is included so Oath guest ELFs link static musl via `/bin/musl-cc`.

Grok-class: borrowed prebuilt, no rustup, no self-update. Removable.
EOF

chmod -R u+rwX "$out"
echo "packed rustc -> $out"
