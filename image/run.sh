#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OATH_BUILD:-$ROOT/build}"
KERNEL="${1:-$OUT/bzImage}"
IMG="${2:-$OUT/oath.qcow2}"
INITRD="$OUT/initrd.gz"

if [ ! -f "$KERNEL" ] || [ ! -f "$IMG" ]; then
  echo "missing image — run image/build.sh first" >&2
  exit 1
fi

QEMU="${QEMU:-qemu-system-x86_64}"
if ! command -v "$QEMU" >/dev/null; then
  TOOLS="$(nix-build "$ROOT/image/tools.nix" --no-out-link)"
  export PATH="$TOOLS/bin:$PATH"
  QEMU=qemu-system-x86_64
fi
if ! command -v "$QEMU" >/dev/null; then
  echo "qemu-system-x86_64 not on PATH (try: nix-shell)" >&2
  exit 1
fi

ACCEL=()
if [ -r /dev/kvm ]; then
  ACCEL=(-enable-kvm)
fi

exec "$QEMU" \
  -machine q35 \
  "${ACCEL[@]}" \
  -m 512 \
  -nographic \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "console=ttyS0 panic=10" \
  -drive "file=$IMG,if=virtio,format=qcow2,cache=writeback" \
  -no-reboot
