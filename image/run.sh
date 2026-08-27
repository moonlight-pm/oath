#!/usr/bin/env bash
# Boot the appliance. Always writes a run directory under build/runs/.
# Interactive serial on stdio; a full copy is also in $RUN/serial.log.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=lib.sh
source "$ROOT/image/lib.sh"
OUT="${OATH_BUILD:-$ROOT/build}"
KERNEL="${OATH_KERNEL:-$OUT/bzImage}"
BACKING="${OATH_IMAGE:-$OUT/oath.qcow2}"
INITRD="$OUT/initrd.gz"

if [ ! -f "$KERNEL" ] || [ ! -f "$BACKING" ] || [ ! -f "$INITRD" ]; then
  echo "missing image — run image/build.sh first" >&2
  exit 1
fi

oath_ensure_qemu "$ROOT"

RUN="${OATH_RUN_DIR:-$(oath_new_run "$OUT" "int")}"
export OATH_RUN_DIR="$RUN"
OVERLAY="${OATH_DISK:-$RUN/disk.qcow2}"
if [ ! -f "$OVERLAY" ]; then
  qemu-img create -f qcow2 -F qcow2 -b "$(realpath "$BACKING")" "$OVERLAY" >/dev/null
fi
oath_write_meta "$RUN" "$KERNEL" "$INITRD" "$BACKING" "$OVERLAY"
{
  echo "QEMU=$QEMU"
  echo "kernel=$KERNEL"
  echo "disk=$OVERLAY"
} >"$RUN/qemu.cmd"

echo "run: $RUN" >&2
echo "serial log: $RUN/serial.log" >&2

ACCEL=()
if [ -r /dev/kvm ]; then
  ACCEL=(-enable-kvm)
fi

# stdio + logfile on the same chardev so interactive and analysis share one serial.
set +e
"$QEMU" \
  -machine q35 \
  "${ACCEL[@]}" \
  -m 512 \
  -display none \
  -monitor none \
  -chardev "stdio,id=cons,logfile=${RUN}/serial.log,signal=off" \
  -serial chardev:cons \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "console=ttyS0 panic=10" \
  -drive "file=$OVERLAY,if=virtio,format=qcow2,cache=writeback" \
  -d guest_errors \
  -D "$RUN/qemu.log" \
  -no-reboot
rc=$?
set -e
python3 - "$RUN" "$rc" <<'PY'
import json, sys, time
d, rc = sys.argv[1], int(sys.argv[2])
p = d + "/meta.json"
try:
    meta = json.load(open(p))
except Exception:
    meta = {}
meta["ended"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
meta["qemu_exit"] = rc
json.dump(meta, open(p, "w"), indent=2)
open(p, "a").write("\n")
PY
echo "qemu exit $rc  (logs in $RUN)" >&2
exit "$rc"
