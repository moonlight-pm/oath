#!/bin/sh
# Bridge compositor clipboard → rootful Xwayland CLIPBOARD.
# Rootful `Xwayland :N -decorate` does not share selections with Wayland.
# Ctrl+V in Steam (and any other X client on that display) then pastes.
xw=/oath/store/pkg/xwayland
export PATH="$xw/libexec:/bin"
export LD_LIBRARY_PATH="$xw/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
export DISPLAY="${DISPLAY:-:2}"

num=${DISPLAY#:}
num=${num%%.*}
pidfile=/tmp/xwayland-clip-$num.pid
logfile=/tmp/xwayland-clip.log

seed() {
	tmp=$(mktemp /tmp/xwayland-clip.XXXXXX) || return 0
	wl-paste -n --type text >"$tmp" 2>/dev/null || true
	if [ -s "$tmp" ]; then
		xclip -selection clipboard -in <"$tmp" 2>/dev/null || true
	fi
	rm -f "$tmp"
}

running() {
	[ -f "$pidfile" ] || return 1
	old=$(cat "$pidfile" 2>/dev/null || true)
	[ -n "$old" ] && kill -0 "$old" 2>/dev/null
}

case "${1-}" in
--seed)
	seed
	exit 0
	;;
--daemon | -d | "")
	if running; then
		seed
		exit 0
	fi
	seed
	wl-paste -n --type text --watch "$xw/libexec/xwayland-clip-in" >>"$logfile" 2>&1 &
	echo $! >"$pidfile"
	exit 0
	;;
*)
	echo "usage: xwayland-clip [--daemon|--seed]" >&2
	exit 2
	;;
esac
