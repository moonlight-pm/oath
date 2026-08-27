# shellcheck shell=bash
# Shared host helpers for run/probe. Source from image/*.sh.

oath_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
}

oath_ensure_qemu() {
  local root="$1"
  QEMU="${QEMU:-qemu-system-x86_64}"
  if ! command -v "$QEMU" >/dev/null; then
    local tools
    tools="$(nix-build "$root/image/tools.nix" --no-out-link)"
    export PATH="$tools/bin:$PATH"
    QEMU=qemu-system-x86_64
  fi
  if ! command -v "$QEMU" >/dev/null; then
    echo "qemu-system-x86_64 not on PATH (try: nix-shell)" >&2
    return 1
  fi
  if ! command -v qemu-img >/dev/null; then
    echo "qemu-img not on PATH" >&2
    return 1
  fi
}

oath_new_run() {
  local out="$1"
  local label="${2:-run}"
  local id
  id="$(date -u +%Y%m%dT%H%M%SZ)-${label}-$$"
  local dir="$out/runs/$id"
  mkdir -p "$dir"
  echo "$dir"
}

oath_sha() {
  if command -v sha256sum >/dev/null; then
    sha256sum "$1" | awk '{print $1}'
  else
    echo "unknown"
  fi
}

oath_write_meta() {
  local dir="$1"
  shift
  # remaining: kernel initrd backing overlay
  local kernel="$1" initrd="$2" backing="$3" overlay="$4"
  python3 - "$dir" "$kernel" "$initrd" "$backing" "$overlay" "${QEMU:-qemu-system-x86_64}" <<'PY'
import json, os, sys, time
d, kernel, initrd, backing, overlay, qemu = sys.argv[1:7]
def sha(p):
    try:
        import hashlib
        h = hashlib.sha256()
        with open(p, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return None
meta = {
    "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "cwd": os.getcwd(),
    "qemu": qemu,
    "kvm": os.access("/dev/kvm", os.R_OK),
    "kernel": kernel,
    "initrd": initrd,
    "backing": backing,
    "overlay": overlay,
    "sha256": {
        "kernel": sha(kernel),
        "initrd": sha(initrd),
        "backing": sha(backing),
    },
}
with open(os.path.join(d, "meta.json"), "w") as f:
    json.dump(meta, f, indent=2)
    f.write("\n")
PY
}
