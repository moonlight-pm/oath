#!/bin/bash
# keyboard-shortcuts-inhibit in rivulet, end to end (Oath): test/inhibit.c,
# a window manager and a client in one process, against a headless rivulet
# on the pixman renderer. Run on an Oath build host after `cargo make pkgs
# rivulet` (it needs the desk's libraries in /lib, wayland-scanner, and the
# build store's wayland-protocols):
#
#   apps/rivulet/test/inhibit.sh [rivulet]
#
# Without an argument it runs the locked pkg:rivulet from the build store.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../.." && pwd)
store=${OATH_STORE:-${XDG_DATA_HOME:-$HOME/.local/share}/oath/store}
locked() { python3 -I -c "import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]]['product'])" \
	"$repo/pkgs/lock.json" "$1"; }

rivulet=${1:-$store/pkg/rivulet/$(locked rivulet)/bin/rivulet}
protocols=$(pkg-config --variable=pkgdatadir wayland-protocols 2>/dev/null ||
	echo "$store/pkg/wayland-protocols/$(locked wayland-protocols)/share/wayland-protocols")
[ -x "$rivulet" ] || { echo "no rivulet at $rivulet"; exit 1; }
[ -d "$protocols" ] || { echo "no wayland-protocols at $protocols"; exit 1; }

tmp=$(mktemp -d)
trap '[ -n "${rpid:-}" ] && kill -9 "$rpid" 2>/dev/null; rm -rf "$tmp"' EXIT
for xml in \
	"$here"/protocol/river-window-management-v1.xml \
	"$here"/protocol/river-xkb-bindings-v1.xml \
	"$here"/protocol/river-layer-shell-v1.xml \
	"$here"/protocol/wlr-layer-shell-unstable-v1.xml \
	"$here"/protocol/virtual-keyboard-unstable-v1.xml \
	"$here"/protocol/wlr-virtual-pointer-unstable-v1.xml \
	"$protocols"/unstable/keyboard-shortcuts-inhibit/keyboard-shortcuts-inhibit-unstable-v1.xml \
	"$protocols"/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml \
	"$protocols"/stable/xdg-shell/xdg-shell.xml; do
	name=$(basename "$xml" .xml)
	wayland-scanner client-header "$xml" "$tmp/$name-client-protocol.h"
	wayland-scanner private-code "$xml" "$tmp/$name-protocol.c"
done
# shellcheck disable=SC2046
cc -std=c11 -O1 -g -Wall -I"$tmp" -o "$tmp/inhibit" "$here/test/inhibit.c" "$tmp"/*-protocol.c \
	$(pkg-config --cflags --libs wayland-client xkbcommon)

export XDG_RUNTIME_DIR=$tmp/run
mkdir -m 700 "$XDG_RUNTIME_DIR"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1
unset WAYLAND_DISPLAY DISPLAY WAYLAND_SOCKET
"$rivulet" -log-level debug -c ':' >"$tmp/rivulet.log" 2>&1 &
rpid=$!
sock=
for _ in $(seq 100); do
	sock=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1) || true
	[ -n "$sock" ] && break
	sleep 0.1
done
[ -n "$sock" ] || { echo "rivulet made no socket"; tail -20 "$tmp/rivulet.log"; exit 1; }

rc=0
WAYLAND_DISPLAY=$sock timeout 60 "$tmp/inhibit" || rc=$?
if [ "$rc" -ne 0 ]; then
	echo "-- rivulet.log (input)"
	grep -iE "inhibit|constraint|binding|focus|error" "$tmp/rivulet.log" | tail -30
fi
exit "$rc"
