#!/bin/sh
# Guest cargo build of sola-arcade only (no alsa.pc / librespot).
set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
sola_src=${SOLA_SRC:-/home/workspaces/sola}
target=${CARGO_TARGET_DIR:-$root/build/sola-target}
bins=${SOLA_BINS:-$target/release}

if [ ! -f "$sola_src/Cargo.toml" ]; then
	echo "build-sola-arcade: missing $sola_src/Cargo.toml" >&2
	exit 1
fi

mkdir -p "$target"
cache=${XDG_CACHE_HOME:-$root/build/sola-cache}
mkdir -p "$cache/sola"
export XDG_CACHE_HOME=$cache
export CARGO_TARGET_DIR=$target
export CARGO_HOME=${CARGO_HOME:-$root/build/cargo-home}
mkdir -p "$CARGO_HOME"
export CC=${CC:-/bin/cc}
export CXX=${CXX:-/bin/c++}
export AR=${AR:-/bin/ar}
export CMAKE_GENERATOR=${CMAKE_GENERATOR:-Ninja}

ld=$root/image/rustc-gnu-ld.sh
chmod +x "$ld" "$root/image/oath-cc-link.sh" 2>/dev/null || true
export OATH_LLD_DIR=$root/build/oath-lld
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$root/image/oath-cc-link.sh
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
rlib=/oath/store/pkg/rustc/lib
zlib=/oath/store/pkg/git/lib
mkdir -p "$OATH_LLD_DIR"
cp -aL /oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/ld.lld "$OATH_LLD_DIR/ld.lld"
cp -aL /oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld "$OATH_LLD_DIR/rust-lld"
chmod u+w "$OATH_LLD_DIR/ld.lld" "$OATH_LLD_DIR/rust-lld"
run_pe() {
	"$interp" --library-path "$glibc" /oath/store/pkg/cc/libexec/patchelf "$@"
}
run_pe --set-interpreter "$interp" "$OATH_LLD_DIR/ld.lld" || true
run_pe --set-rpath "$rlib:$glibc:$zlib" "$OATH_LLD_DIR/ld.lld" || true
run_pe --set-interpreter "$interp" "$OATH_LLD_DIR/rust-lld" || true
run_pe --set-rpath "$rlib:$glibc:$zlib" "$OATH_LLD_DIR/rust-lld" || true

for soname in libc.so:libc.so.6 libm.so:libm.so.6 libpthread.so:libpthread.so.0 \
	libdl.so:libdl.so.2 librt.so:librt.so.1 libutil.so:libc.so.6 \
	libgcc_s.so:libgcc_s.so.1; do
	link=${soname%%:*}
	tgt=${soname#*:}
	if [ ! -e /oath/store/pkg/glibc/lib/"$link" ]; then
		sudo -n ln -sfn "$tgt" /oath/store/pkg/glibc/lib/"$link"
	fi
done

sola_cfg=$sola_src/.cargo/config.toml
if [ -f "$sola_cfg" ] && [ ! -f "$sola_cfg.oath-bak" ]; then
	cp "$sola_cfg" "$sola_cfg.oath-bak"
fi
if [ -f "$sola_cfg" ]; then
	cat >"$sola_cfg" <<'EOF'
[alias]
make = "run -q -p sola-make --"

[target.x86_64-unknown-linux-gnu]
rustflags = []
EOF
fi
unset RUSTFLAGS || true

st=0
(cd "$sola_src" && cargo build --release -p sola-arcade) || st=$?
if [ -f "$sola_src/.cargo/config.toml.oath-bak" ]; then
	mv "$sola_src/.cargo/config.toml.oath-bak" "$sola_src/.cargo/config.toml"
fi
[ "$st" = 0 ] || exit "$st"
test -f "$bins/sola-arcade"
echo "built $bins/sola-arcade"
