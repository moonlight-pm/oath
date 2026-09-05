#!/bin/sh
# Live-install remaining Sola kit ELFs into pkg:sola on this Oath box.
# Guest cargo build from the Sola tree, patchelf, wrappers, /bin links.
# Busybox ash. Needs cargo, rustc, cc, cmake, patchelf, sudo.
set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
sola_src=${SOLA_SRC:-/home/workspaces/sola}
target=${CARGO_TARGET_DIR:-$root/build/sola-target}
bins=${SOLA_BINS:-$target/release}
store=/oath/store/pkg/sola
cef=/oath/store/pkg/sola/cef
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
rpath="/oath/store/pkg/glibc/lib:/oath/store/pkg/river/lib:/oath/store/pkg/sola/lib:/oath/store/pkg/sola/cef/Release:/oath/store/pkg/pipewire/lib"
browser_rpath="/oath/store/pkg/glibc/lib:/oath/store/pkg/sola/lib:/oath/store/pkg/sola/cef/Release:/oath/store/pkg/river/lib:/oath/store/pkg/pipewire/lib"

apps="sola-settings sola-monitor sola-kit sola-preview sola-paint sola-mail sola-arcade sola-scope sola-spotify sola-wrapper"

as_root() {
	if [ "$(id -u)" = 0 ]; then
		"$@"
	else
		sudo -n "$@"
	fi
}

if [ ! -f "$sola_src/Cargo.toml" ]; then
	echo "install-sola-kit: missing $sola_src/Cargo.toml (set SOLA_SRC)" >&2
	exit 1
fi
if [ ! -f "$cef/Release/libcef.so" ]; then
	echo "install-sola-kit: missing $cef/Release/libcef.so" >&2
	exit 1
fi
if [ ! -x /bin/cargo ] || [ ! -x /bin/patchelf ]; then
	echo "install-sola-kit: need /bin/cargo and /bin/patchelf (T35)" >&2
	exit 1
fi

if [ "${SKIP_BUILD:-}" != 1 ]; then
	echo "==> cargo build kit apps ($sola_src)"
	mkdir -p "$target"
	cache=${XDG_CACHE_HOME:-$root/build/sola-cache}
	mkdir -p "$cache/sola"
	cefver=$(tr -d '\n' <"$sola_src/cef-version")
	ln -sfn "$cef" "$cache/sola/cef-$cefver"
	export XDG_CACHE_HOME=$cache
	export CARGO_TARGET_DIR=$target
	export CARGO_HOME=${CARGO_HOME:-$root/build/cargo-home}
	mkdir -p "$CARGO_HOME"
	export CC=${CC:-/bin/cc}
	export CXX=${CXX:-/bin/c++}
	export AR=${AR:-/bin/ar}
	export CMAKE_GENERATOR=${CMAKE_GENERATOR:-Ninja}
	# Zig 0.16 + rustc -fuse-ld=lld: live-patch pkg:cc wrappers if
	# they still pass every flag through.
	if ! grep -q -- '--target=' /oath/store/pkg/cc/libexec/zig-gnu-cc.sh 2>/dev/null; then
		echo "==> patch pkg:cc wrappers (compile vs link)"
		as_root cp "$here/zig-gnu-cc.sh" /oath/store/pkg/cc/libexec/zig-gnu-cc.sh
		as_root chmod 755 /oath/store/pkg/cc/libexec/zig-gnu-cc.sh
		for n in cc gcc; do
			printf '%s\n' '#!/bin/sh' 'exec /oath/store/pkg/cc/libexec/zig-gnu-cc.sh cc "$@"' | as_root tee "/oath/store/pkg/cc/bin/$n" >/dev/null
			as_root chmod 755 "/oath/store/pkg/cc/bin/$n"
		done
		for n in c++ g++; do
			printf '%s\n' '#!/bin/sh' 'exec /oath/store/pkg/cc/libexec/zig-gnu-cc.sh c++ "$@"' | as_root tee "/oath/store/pkg/cc/bin/$n" >/dev/null
			as_root chmod 755 "/oath/store/pkg/cc/bin/$n"
		done
	fi
	# rustc host link: rust-lld + pkg:glibc (not Zig cc).
	ld=$root/image/rustc-gnu-ld.sh
	chmod +x "$ld" "$here/zig-gnu-cc.sh" 2>/dev/null || true
	export OATH_LLD_DIR=$root/build/oath-lld
	export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$root/image/oath-cc-link.sh
	chmod +x "$root/image/oath-cc-link.sh" "$ld" 2>/dev/null || true
	# Prepare ld.lld + rust-lld once (lld-wrapper keys off argv0 and
	# execs a sibling rust-lld). Do not race this from N cargo jobs.
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
	run_pe --set-interpreter "$interp" "$OATH_LLD_DIR/ld.lld"
	run_pe --set-rpath "$rlib:$glibc:$zlib" "$OATH_LLD_DIR/ld.lld"
	run_pe --set-interpreter "$interp" "$OATH_LLD_DIR/rust-lld"
	run_pe --set-rpath "$rlib:$glibc:$zlib" "$OATH_LLD_DIR/rust-lld"
	echo "==> host linker $OATH_LLD_DIR/ld.lld"
	# Sola .cargo/config.toml bakes NixOS -Wl,-rpath (cc-style).
	# rust-lld must not see those. Restore after cargo.
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
	for soname in libc.so:libc.so.6 libm.so:libm.so.6 libpthread.so:libpthread.so.0 \
		libdl.so:libdl.so.2 librt.so:librt.so.1 libutil.so:libc.so.6 \
		libgcc_s.so:libgcc_s.so.1; do
		link=${soname%%:*}
		tgt=${soname#*:}
		if [ ! -e /oath/store/pkg/glibc/lib/"$link" ]; then
			as_root ln -sfn "$tgt" /oath/store/pkg/glibc/lib/"$link"
		fi
	done
	pkgs=""
	for b in $apps; do
		pkgs="$pkgs -p $b"
	done
	# shellcheck disable=SC2086
	cargo_st=0
	(cd "$sola_src" && cargo build --release $pkgs) || cargo_st=$?
	if [ -f "$sola_src/.cargo/config.toml.oath-bak" ]; then
		mv "$sola_src/.cargo/config.toml.oath-bak" "$sola_src/.cargo/config.toml"
	fi
	[ "$cargo_st" = 0 ] || exit "$cargo_st"
fi

missing=""
for b in $apps; do
	if [ ! -f "$bins/$b" ]; then
		missing="$missing $b"
	fi
done
if [ -n "$missing" ]; then
	echo "install-sola-kit: missing ELFs in $bins:$missing" >&2
	exit 1
fi

as_root mkdir -p "$store/bin" "$store/libexec" "$store/lib"

guest_env='export PATH=/bin
export HOME="${HOME:-/home}"
cd "$HOME" || true
export SHELL="${SHELL:-/bin/thoxa}"
export LANG=C.UTF-8
export LC_ALL=C.UTF-8
export LOCALE_ARCHIVE=/oath/store/pkg/sola/lib/locale/locale-archive
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
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
export SOLA_OUTPUT_PICK=preferred'

for b in $apps; do
	echo "==> $b"
	as_root cp -a "$bins/$b" "$store/libexec/$b"
	as_root chmod u+w "$store/libexec/$b"
	as_root chmod +x "$store/libexec/$b"
	rp=$rpath
	if [ "$b" = sola-wrapper ]; then
		rp=$browser_rpath
	fi
	as_root patchelf --set-interpreter "$interp" "$store/libexec/$b" || true
	as_root patchelf --set-rpath "$rp" "$store/libexec/$b"
	wrap=$store/bin/$b
	as_root tee "$wrap" >/dev/null <<WRAP
#!/bin/sh
$guest_env
/bin/mkdir -p /tmp/fontconfig /oath/log "\$HOME/.local/share" "\$HOME/.config"
exec /oath/store/pkg/sola/libexec/$b "\$@" >>/oath/log/$b.log 2>&1
WRAP
	as_root chmod +x "$wrap"
	as_root ln -sfn "$wrap" "/bin/$b"
done

echo "==> kit apps linked"
for b in $apps; do
	test -x "/bin/$b" && echo "  /bin/$b"
done
