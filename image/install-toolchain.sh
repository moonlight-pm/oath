#!/bin/sh
# Live-install pkg:cc, pkg:rustc, pkg:cmake, pkg:pkg-config on this Oath
# box. Fetch official tarballs, fill the store, catalog objects, apply.
# Busybox ash. Needs curl, tar, xz, unzip, sudo, oath.
set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
fetchdir=${OATH_FETCH:-$root/build/fetch}
stagedir=${OATH_STAGE:-$root/build/toolchain}
store=/oath/store/pkg
objects=/oath/objects/pkg

mkdir -p "$fetchdir" "$stagedir"
export OATH_FETCH=$fetchdir

if [ -x "$stagedir/cc/libexec/zig/zig" ] && [ -x "$stagedir/cc/libexec/patchelf" ]; then
	echo "==> pkg:cc already packed"
else
	echo "==> pkg:cc (zig + patchelf)"
	sh "$here/pack-cc.sh" "$stagedir/cc"
fi
export PATCHELF=$stagedir/cc/libexec/patchelf

if [ -x "$stagedir/pkg-config/bin/pkg-config" ]; then
	echo "==> pkg:pkg-config already packed"
else
	echo "==> pkg:pkg-config"
	sh "$here/pack-pkg-config.sh" "$stagedir/pkg-config"
fi

if [ -x "$stagedir/cmake/libexec/cmake" ]; then
	echo "==> pkg:cmake already packed"
else
	echo "==> pkg:cmake"
	sh "$here/pack-cmake.sh" "$stagedir/cmake"
fi

if [ -x "$stagedir/rustc/libexec/rustc" ] || [ -x "$stagedir/rustc/bin/rustc" ]; then
	echo "==> pkg:rustc already packed"
else
	echo "==> pkg:rustc"
	sh "$here/pack-rustc.sh" "$stagedir/rustc"
fi

as_root() {
	if [ "$(id -u)" = 0 ]; then
		"$@"
	else
		sudo -n "$@"
	fi
}

write_obj() {
	name=$1
	dir=$objects/$name
	as_root mkdir -p "$dir"
	printf '%s\n' '{
  "present": true
}' | as_root tee "$dir/desired.json" >/dev/null
	printf '%s\n' '{
  "present": false,
  "links": [],
  "removable": true
}' | as_root tee "$dir/actual.json" >/dev/null
	printf '%s\n' "{
  \"id\": \"pkg:$name\",
  \"kind\": \"pkg\",
  \"name\": \"$name\",
  \"safety\": \"mutate\",
  \"status\": \"drift\"
}" | as_root tee "$dir/meta.json" >/dev/null
}

install_pkg() {
	name=$1
	echo "==> install pkg:$name"
	as_root rm -rf "$store/$name"
	as_root mkdir -p "$store"
	as_root cp -a "$stagedir/$name" "$store/$name"
	as_root chmod -R u+rX "$store/$name"
	write_obj "$name"
}

install_pkg cc
install_pkg pkg-config
install_pkg cmake
install_pkg rustc

echo "==> host.env toolchain"
env_json='{"env":{"GROK_DISABLE_AUTOUPDATER":"1","SHELL":"/bin/thoxa","THOXA_ROOT":"/oath/store/pkg/thoxa","CC":"/bin/cc","CXX":"/bin/c++","AR":"/bin/ar","CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER":"/bin/musl-cc","CMAKE_GENERATOR":"Ninja"}}'
if [ "$(id -u)" = 0 ]; then
	oath set host:local --from-json "$env_json"
	oath apply pkg:cc pkg:pkg-config pkg:cmake pkg:rustc host:local
else
	sudo -n oath set host:local --from-json "$env_json"
	sudo -n oath apply pkg:cc pkg:pkg-config pkg:cmake pkg:rustc host:local
fi

echo "==> courage"
for b in cc musl-cc ar ranlib patchelf rustc cargo cmake ninja pkg-config; do
	p=$(command -v "$b" || true)
	echo "  $b ${p:-MISSING}"
done
echo 'int main(void){return 0;}' >/tmp/oath-cc-probe.c
cc -o /tmp/oath-cc-probe /tmp/oath-cc-probe.c && /tmp/oath-cc-probe && echo "  cc gnu link ok" || echo "  cc gnu link FAIL"
rustc --version || true
cargo --version || true
cmake --version | head -n1 || true
ninja --version || true
pkg-config --version || true
