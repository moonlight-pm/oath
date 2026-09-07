#!/bin/sh
# Old shortcut: Ubuntu mainline 7.3-rc1 debs. Panicked on canto.
# Do not use. Compile vanilla with image/build-linux.sh instead.
#
#   sh image/fetch-linux-mainline.sh [outdir]
#
# Then:
#   OATH_KERNEL=$out/vmlinuz OATH_MODULES=$out/modules OATH_BUSYBOX=/bin/busybox \
#     cargo make esp --esp /dev/sda1 --confirm --root /dev/sda2
set -eu

out=${1:-$(CDPATH= cd -- "$(dirname "$0")/../build/linux" && pwd)}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
fetch=$here/fetch-url.sh
cache=${OATH_FETCH:-$(CDPATH= cd -- "$here/../build/fetch" && pwd)}
zstd=${ZSTD:-/tmp/zstd}
ar_bin=/oath/store/pkg/cc/libexec/zig/zig

KVER=7.3.0-070300rc1-generic
BUILD=7.3.0-070300rc1.202608310003
BASE=https://kernel.ubuntu.com/mainline/v7.3-rc1/amd64
IMG=linux-image-unsigned-${KVER}_${BUILD}_amd64.deb
MOD=linux-modules-${KVER}_${BUILD}_amd64.deb
IMG_SHA=3f2a1433f527f41745997c04bc9845055c289aac64bee9a50f403770085a4658
MOD_SHA=d64e02764f84913916fde07adb2da578fb082ef1568d174c6d05ce2a6f9cf9e6

mkdir -p "$cache" "$out"
sh "$fetch" "$BASE/$IMG" "$cache/$IMG" "$IMG_SHA"
sh "$fetch" "$BASE/$MOD" "$cache/$MOD" "$MOD_SHA"

extract_deb() {
	deb=$1 dest=$2
	tmp=$(mktemp -d)
	(cd "$tmp" && "$ar_bin" ar x "$deb")
	mkdir -p "$dest"
	if [ -f "$tmp/data.tar.zst" ]; then
		"$zstd" -d "$tmp/data.tar.zst" -c | tar -x -C "$dest"
	elif [ -f "$tmp/data.tar.xz" ]; then
		tar -xJf "$tmp/data.tar.xz" -C "$dest"
	elif [ -f "$tmp/data.tar.gz" ]; then
		tar -xzf "$tmp/data.tar.gz" -C "$dest"
	elif [ -f "$tmp/data.tar" ]; then
		tar -xf "$tmp/data.tar" -C "$dest"
	else
		echo "extract_deb: no data.tar in $deb" >&2
		ls -l "$tmp" >&2
		rm -rf "$tmp"
		return 1
	fi
	rm -rf "$tmp"
}

stage=$(mktemp -d)
extract_deb "$cache/$IMG" "$stage/image"
extract_deb "$cache/$MOD" "$stage/modules"
cp -a "$stage/image/boot/vmlinuz-$KVER" "$out/vmlinuz"
mkdir -p "$out/modules"
# Ubuntu merged /lib → /usr/lib.
if [ -d "$stage/modules/usr/lib/modules/$KVER" ]; then
	cp -a "$stage/modules/usr/lib/modules/$KVER" "$out/modules/$KVER"
elif [ -d "$stage/modules/lib/modules/$KVER" ]; then
	cp -a "$stage/modules/lib/modules/$KVER" "$out/modules/$KVER"
else
	echo "no modules tree for $KVER" >&2
	find "$stage/modules" -maxdepth 4 -type d >&2
	rm -rf "$stage"
	exit 1
fi
rm -rf "$stage"
echo "kernel $out/vmlinuz"
echo "modules $out/modules/$KVER"
echo "OATH_KERNEL=$out/vmlinuz"
echo "OATH_MODULES=$out/modules"
echo "OATH_KVER=$KVER"
