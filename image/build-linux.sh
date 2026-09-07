#!/bin/sh
# Compile vanilla kernel.org Linux with the Oath fragment (not Ubuntu generic).
#
#   sh image/build-linux.sh [outdir]
#
# CPU: nice(10) + half the cores (this desk is 32, so -j16). Override with
# OATH_LINUX_JOBS. Then:
#   OATH_KERNEL=$out/vmlinuz OATH_MODULES=$out/modules \
#     cargo make esp --esp /dev/sda1 --confirm --root /dev/sda2
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
here=$root/image
out=${1:-$root/build/linux}
src=${OATH_LINUX_SRC:-$root/build/linux-src}
jobs=${OATH_LINUX_JOBS:-}
kver=${OATH_LINUX_KVER:-7.3-rc1}
fetch=$here/fetch-url.sh
cache=${OATH_FETCH:-$root/build/fetch}
fragment=$here/linux.fragment

if [ -z "$jobs" ]; then
	n=$(nproc)
	jobs=$((n / 2))
	[ "$jobs" -ge 1 ] || jobs=1
fi

mkdir -p "$cache" "$out" "$src"

tarball=$cache/linux-$kver.tar.gz
url=${OATH_LINUX_URL:-https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/snapshot/linux-$kver.tar.gz}

if [ ! -f "$src/Makefile" ]; then
	if [ ! -f "$tarball" ]; then
		sh "$fetch" "$url" "$tarball"
	fi
	echo "extract $tarball"
	rm -rf "$src"
	mkdir -p "$src"
	tar -xzf "$tarball" -C "$src" --strip-components=1
fi

cd "$src"
patches=$here/linux-patches
if [ -d "$patches" ]; then
	for p in "$patches"/*.patch; do
		[ -f "$p" ] || continue
		base=$(basename "$p")
		if patch -p1 -N --dry-run -r - --forward -i "$p" >/dev/null 2>&1; then
			echo "patch $base"
			patch -p1 -N -r - --forward -i "$p"
		elif patch -p1 -R --dry-run -r - -i "$p" >/dev/null 2>&1; then
			echo "patch $base (already applied)"
		else
			echo "patch $base FAILED" >&2
			exit 1
		fi
	done
fi
echo "config fragment $fragment  jobs=$jobs  nice=10"
make ARCH=x86_64 defconfig
./scripts/kconfig/merge_config.sh -m .config "$fragment"
make ARCH=x86_64 olddefconfig

echo "compile bzImage + modules (nice 10, -j$jobs)"
nice -n 10 ionice -c2 -n4 make ARCH=x86_64 -j"$jobs" bzImage modules

rel=$(make -s ARCH=x86_64 kernelrelease)
moddest=$out/modules/$rel
rm -rf "$moddest"
mkdir -p "$moddest"
nice -n 10 make ARCH=x86_64 INSTALL_MOD_PATH="$out/modules-root" INSTALL_MOD_STRIP=1 modules_install
# modules_install writes lib/modules/<rel>
rm -rf "$out/modules"
mkdir -p "$out/modules"
mv "$out/modules-root/lib/modules/$rel" "$out/modules/$rel"
rm -rf "$out/modules-root"
cp -a "$src/arch/x86/boot/bzImage" "$out/vmlinuz"

# Pitcairn firmware next to the kernel so `cargo make boot` packs it
# without nix-build tools.nix (OATH_KERNEL parent dir).
if [ ! -d "$out/firmware/amdgpu" ]; then
	src_fw=${OATH_FIRMWARE:-}
	if [ -z "$src_fw" ] || [ ! -d "$src_fw" ]; then
		src_fw=$here/../build/linux/firmware
	fi
	if [ -d "$src_fw/amdgpu" ]; then
		mkdir -p "$out/firmware"
		cp -a "$src_fw/." "$out/firmware/"
	fi
fi

echo "kernel $out/vmlinuz"
echo "modules $out/modules/$rel"
echo "OATH_KERNEL=$out/vmlinuz"
echo "OATH_MODULES=$out/modules"
echo "OATH_KVER=$rel"
if [ -d "$out/firmware/amdgpu" ]; then
	echo "OATH_FIRMWARE=$out/firmware"
fi
