#!/usr/bin/env bash
# Smoke test of the built pkg:rivulet, in the build sandbox on the build
# host: rivulet runs headless on the pixman renderer, and its init
# command, an X client (xkbcomp), starts Xwayland and reads the keymap
# back. Run from the repo root after `cargo make pkgs rivulet`:
#
#   apps/rivulet/test/sandbox.sh
#
# It writes build/rivulet-smoke.plan (rivulet's run closure and the base
# packs, pinned from pkgs/lock.json) and builds it as pkg:rivulet-smoke
# in .cache/oath; the product holds the logs.
set -eu
cd "$(dirname "$0")/../../.."
plan=build/rivulet-smoke.plan
mkdir -p build
python3 -I - "$plan" <<'EOF'
import json, os, sys, tomllib
lock = json.load(open("pkgs/lock.json"))
base = ["gcc", "glibc", "bash", "coreutils", "sed", "grep", "gawk", "findutils",
        "diffutils", "patch", "tar", "gzip", "xz", "bzip2", "make"]
def run(n):
    p = f"pkgs/{n}/pkg.toml"
    return tomllib.load(open(p, "rb")).get("run", []) if os.path.isfile(p) else []
seen = set()
def visit(n):
    if n not in seen:
        seen.add(n)
        for d in run(n):
            visit(d)
for n in ["rivulet"] + base:
    visit(n)
plan = {
    "name": "rivulet-smoke",
    "produces": "pkg:rivulet-smoke",
    "build_needs": [{"id": f"pkg:{n}", "hash": lock[n]["product"]} for n in sorted(seen)],
    "run_needs": [],
}
# The sandbox has no /dev/shm (wlroots' shm buffers) and no /etc/passwd
# (Xwayland admits local clients by user name); a machine has both.
script = r'''#!/bin/sh
set -eu
mkdir -p /out /dev/shm /etc
echo "home:x:$(id -u):$(id -g)::/tmp/home:/bin/sh" >/etc/passwd
export XDG_RUNTIME_DIR=/tmp/run HOME=/tmp/home XDG_CONFIG_HOME=/tmp/home/.config
mkdir -p -m 700 "$XDG_RUNTIME_DIR" "$HOME"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1
rivulet -version >/out/version 2>&1 || true
rivulet -log-level debug -c 'timeout 20 xkbcomp "$DISPLAY" /out/keymap.xkb >/out/xkbcomp.txt 2>&1; echo "xkbcomp exit $?" >>/out/xkbcomp.txt' >/out/rivulet.log 2>&1 &
pid=$!
sleep 4
kill -TERM $pid || true
rc=0
wait $pid || rc=$?
echo "rivulet exit $rc" >>/out/version
'''
open(sys.argv[1], "w").write("#!oath-plan\n" + json.dumps(plan, indent=2) + "\n---\n" + script)
EOF
cargo build -q -p oath
product=$(target/debug/oath --root .cache/oath build --file "$plan" 2>/dev/null | awk '/^pkg:rivulet-smoke /{print $2}')
out=.cache/oath/store/pkg/rivulet-smoke/$product
cat "$out/version" "$out/xkbcomp.txt"
grep -q 'Xserver is ready' "$out/rivulet.log" && echo "Xwayland: ready"
echo "keymap: $(wc -l <"$out/keymap.xkb" 2>/dev/null || echo 0) lines; logs in $out"
grep -q '^rivulet exit 0$' "$out/version" && grep -q '^xkbcomp exit 0$' "$out/xkbcomp.txt"
