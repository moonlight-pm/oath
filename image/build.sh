#!/usr/bin/env bash
# Assemble a btrfs qcow2 + initramfs. Borrowed kernel/busybox; our PID 1 + oath.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OATH_BUILD:-$ROOT/build}"
mkdir -p "$OUT"

need() { command -v "$1" >/dev/null || { echo "missing $1" >&2; exit 1; }; }

if [ -z "${OATH_KERNEL:-}" ] || [ -z "${OATH_MODULES:-}" ] || [ -z "${OATH_BUSYBOX:-}" ]; then
  echo "loading tools via nix-build image/tools.nix ..."
  TOOLS="$(nix-build "$ROOT/image/tools.nix" --no-out-link)"
  export OATH_KERNEL="${OATH_KERNEL:-$TOOLS/bzImage}"
  export OATH_MODULES="${OATH_MODULES:-$TOOLS/modules}"
  export OATH_BUSYBOX="${OATH_BUSYBOX:-$TOOLS/busybox}"
  export OATH_BTRFS="${OATH_BTRFS:-$TOOLS/btrfs}"
  export PATH="$TOOLS/bin:$PATH"
  if [ -z "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-}" ] && [ -e "$TOOLS/musl-cc" ]; then
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$TOOLS/musl-cc"
  fi
fi

need qemu-img
need mkfs.btrfs
need cargo
need cpio
need xz

echo "kernel=$OATH_KERNEL"
echo "modules=$OATH_MODULES"
echo "busybox=$OATH_BUSYBOX"

echo ">> musl binaries"
cargo build --release --target x86_64-unknown-linux-musl \
  -p oath -p oath-init
BIN="$ROOT/target/x86_64-unknown-linux-musl/release"
test -x "$BIN/oath"
test -x "$BIN/oath-init"
test -x "$BIN/serial-login"

echo ">> initramfs"
IR="$OUT/initramfs"
rm -rf "$IR"
mkdir -p "$IR"/{bin,dev,proc,sys,newroot,lib/modules}
cp "$BIN/oath-init" "$IR/init"
chmod +x "$IR/init"
cp "$OATH_BUSYBOX" "$IR/bin/busybox"
chmod +x "$IR/bin/busybox"
ln -sf busybox "$IR/bin/sh"

KVER="$(ls "$OATH_MODULES" | head -1)"
MDST="$IR/lib/modules/$KVER"
mkdir -p "$MDST"
mods=(
  kernel/drivers/virtio/virtio.ko.xz
  kernel/drivers/virtio/virtio_ring.ko.xz
  kernel/drivers/virtio/virtio_pci_legacy_dev.ko.xz
  kernel/drivers/virtio/virtio_pci_modern_dev.ko.xz
  kernel/drivers/virtio/virtio_pci.ko.xz
  kernel/drivers/block/virtio_blk.ko.xz
  kernel/crypto/crc32c_generic.ko.xz
  kernel/lib/libcrc32c.ko.xz
  kernel/crypto/xor.ko.xz
  kernel/lib/raid6/raid6_pq.ko.xz
  kernel/fs/btrfs/btrfs.ko.xz
)
for m in "${mods[@]}"; do
  src="$OATH_MODULES/$KVER/$m"
  if [ -f "$src" ]; then
    dst="$MDST/${m%.xz}"
    mkdir -p "$(dirname "$dst")"
    xz -d -c "$src" > "$dst"
  else
    echo "warn: missing module $m" >&2
  fi
done

( cd "$IR" && find . | cpio -o -H newc --quiet ) | gzip -9 > "$OUT/initrd.gz"
echo "initrd $OUT/initrd.gz"

echo ">> stage rootfs"
STAGE="$OUT/stage"
rm -rf "$STAGE"
mkdir -p "$STAGE"/{bin,sbin,usr/lib/oath,etc,root,tmp,proc,sys,dev,run,oath,lib}
cp "$OATH_BUSYBOX" "$STAGE/bin/busybox"
chmod +x "$STAGE/bin/busybox"
# Relative applets — busybox --install -s would bake host-absolute links.
( cd "$STAGE/bin" && ./busybox --list | while read -r a; do ln -sf busybox "$a"; done )
if [ -n "${OATH_BTRFS:-}" ] && [ -x "$OATH_BTRFS" ]; then
  cp "$OATH_BTRFS" "$STAGE/bin/btrfs"
  chmod +x "$STAGE/bin/btrfs"
fi
cp "$BIN/oath" "$STAGE/bin/oath"
cp "$BIN/oath-init" "$STAGE/usr/lib/oath/init"
cp "$BIN/serial-login" "$STAGE/usr/lib/oath/serial-login"
chmod +x "$STAGE/bin/oath" "$STAGE/usr/lib/oath/init" "$STAGE/usr/lib/oath/serial-login"
ln -sf ../usr/lib/oath/init "$STAGE/sbin/init"
printf 'root:x:0:0:root:/root:/bin/sh\n' > "$STAGE/etc/passwd"
printf 'root:x:0:\n' > "$STAGE/etc/group"
"$BIN/oath" --root "$STAGE/oath" seed

echo ">> rootfs (btrfs subvol @) — loop-mount needs root"
RAW="$OUT/root.raw"
rm -f "$RAW" "$OUT/oath.qcow2"
qemu-img create -f raw "$RAW" 512M >/dev/null
mkfs.btrfs -q -L oath "$RAW"
MNT="$OUT/mnt"
ROOTFS="$OUT/rootfs"
mkdir -p "$MNT" "$ROOTFS"
sudo -n mount -o loop "$RAW" "$MNT"
sudo -n btrfs subvolume create "$MNT/@" >/dev/null
sudo -n mount -o loop,subvol=@ "$RAW" "$ROOTFS"
sudo -n cp -a "$STAGE"/. "$ROOTFS"/
sudo -n umount "$ROOTFS"
sudo -n umount "$MNT"
qemu-img convert -f raw -O qcow2 "$RAW" "$OUT/oath.qcow2"
rm -f "$RAW"
sudo -n chown -R "$(id -u):$(id -g)" "$OUT" 2>/dev/null || true
rm -f "$OUT/bzImage"
cp "$OATH_KERNEL" "$OUT/bzImage"
echo "image $OUT/oath.qcow2"
echo "kernel $OUT/bzImage"
echo "run: $ROOT/image/run.sh"
