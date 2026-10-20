#!/usr/bin/env bash
# Headless check of rivulet (or a River binary) on the build host, driven by
# unmodified sola-river, and a comparison of two runs (rivulet against
# upstream River).
#
#   nix-shell apps/rivulet/dev.nix --run 'apps/rivulet/test/headless.sh run <compositor> <out>'
#   nix-shell apps/rivulet/dev.nix --run 'apps/rivulet/test/headless.sh compare <out-a> <out-b>'
#
# SOLA_BIN names a directory holding sola-bus, sola-call, sola-river and
# solactl built from Sola (moonlight-pm/Sola). Each run gets its own runtime dir, config,
# home and Sola log dir, so a desk on the same machine is not touched.
set -u

run() {
	local river=$1 out=$2
	local sola=${SOLA_BIN:?set SOLA_BIN to a directory with sola-bus, sola-call, sola-river, solactl}
	local s=$sola/solactl
	rm -rf "$out" && mkdir -p "$out"/{cfg,home,log}
	out=$(cd "$out" && pwd)
	export XDG_RUNTIME_DIR=/run/user/$(id -u)/river-test-$$
	rm -rf "$XDG_RUNTIME_DIR" && mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
	export XDG_CONFIG_HOME=$out/cfg HOME=$out/home SOLA_LOG_DIR=$out/log
	export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1
	export ASAN_OPTIONS="detect_leaks=${LEAKS:-0}:log_path=$out/asan" UBSAN_OPTIONS="print_stacktrace=1:log_path=$out/ubsan"
	unset WAYLAND_DISPLAY DISPLAY WAYLAND_SOCKET

	pids=()
	trap 'for p in "${pids[@]}"; do kill -9 "$p" 2>/dev/null; done; rm -rf "$XDG_RUNTIME_DIR"' EXIT
	step() { echo "== $*" >>"$out/steps.txt"; }

	# A pipe, not a file: Zig's River writes stderr at its own offset and
	# would overwrite Xwayland's lines in a shared file.
	"$river" -log-level debug -c ':' > >(cat >"$out/river.log") 2>&1 &
	local rpid=$! sock=
	for _ in $(seq 100); do
		sock=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)
		[ -n "$sock" ] && break
		sleep 0.1
	done
	[ -z "$sock" ] && { echo "no wayland socket"; tail -20 "$out/river.log"; return 1; }
	export WAYLAND_DISPLAY=$sock
	sleep 0.5

	wayland-info >"$out/wayland-info.txt" 2>&1
	grep -E "^interface" "$out/wayland-info.txt" |
		sed -E "s/^interface: '([^']+)',[[:space:]]*version:[[:space:]]*([0-9]+).*/\1 \2/" >"$out/globals.txt"

	"$sola/sola-bus" >"$out/sola-bus.log" 2>&1 &
	pids+=($!)
	"$sola/sola-call" >"$out/sola-call.log" 2>&1 &
	pids+=($!)
	sleep 0.3
	"$sola/sola-river" >"$out/sola-river.log" 2>&1 &
	local sr=$!
	pids+=($sr)
	sleep 1.5

	step "keyboard: focus a shell and type into it"
	foot -a typed -e bash --norc --noprofile >"$out/foot-typed.log" 2>&1 &
	pids+=($!)
	sleep 2
	"$s" compositor windows >"$out/windows-0.txt" 2>&1
	local wid
	wid=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["typed"][0]["window_id"])' "$out/windows-0.txt")
	"$s" emit Focus "{\"window_id\":$wid}" >>"$out/steps.txt" 2>&1
	sleep 0.5
	for k in c d Return t o u c h space t y p e d Return; do
		"$s" compositor input key "$k" >/dev/null 2>&1
	done
	sleep 1
	"$s" compositor screenshot -o "$out/shot-typed.png" >/dev/null 2>&1
	echo "typed: $(ls "$HOME/typed" 2>&1)" >>"$out/steps.txt"

	step "pointer: move, click, scroll"
	"$s" compositor input move 640 360 >>"$out/steps.txt" 2>&1
	"$s" compositor input click 640 360 >>"$out/steps.txt" 2>&1
	"$s" compositor input scroll -y 5 >>"$out/steps.txt" 2>&1

	step "xwayland: xterm"
	local disp
	disp=$(grep -oE 'Starting Xwayland on :[0-9]+' "$out/river.log" | head -1 | grep -oE ':[0-9]+')
	DISPLAY=$disp xterm -class XTermTest -geometry 60x10 -e sleep 30 >"$out/xterm.log" 2>&1 &
	local xt=$!
	pids+=($xt)
	sleep 3
	"$s" compositor windows >"$out/windows-1.txt" 2>&1

	step "layer shell: swaybg"
	swaybg -c '#336699' >"$out/swaybg.log" 2>&1 &
	pids+=($!)
	sleep 1.5

	step "output management: wlr-randr mode and scale"
	wlr-randr >"$out/randr-1.txt" 2>&1
	wlr-randr --output HEADLESS-1 --custom-mode 1600x900 >>"$out/steps.txt" 2>&1
	sleep 1
	wlr-randr --output HEADLESS-1 --scale 1.5 >>"$out/steps.txt" 2>&1
	sleep 1
	wlr-randr >"$out/randr-2.txt" 2>&1
	wlr-randr --output HEADLESS-1 --scale 1 --custom-mode 1280x720 >>"$out/steps.txt" 2>&1
	sleep 1

	step "fullscreen client"
	foot -a fs --fullscreen -e sleep 30 >"$out/foot-fs.log" 2>&1 &
	local fs=$!
	pids+=($fs)
	sleep 2
	"$s" compositor screenshot -o "$out/shot-fs.png" >/dev/null 2>&1
	kill $fs
	sleep 1

	step "hard kill of mapped clients"
	foot -a victim -e sleep 30 >"$out/foot-victim.log" 2>&1 &
	local v=$!
	pids+=($v)
	sleep 2
	kill -9 $v
	sleep 1
	kill -9 $xt
	sleep 1

	step "six windows at once"
	for i in 1 2 3 4 5 6; do
		foot -a "many$i" -e sleep 3 >/dev/null 2>&1 &
		pids+=($!)
	done
	sleep 2
	"$s" compositor screenshot -o "$out/shot-many.png" >/dev/null 2>&1
	sleep 3

	step "window manager restart"
	kill -9 $sr
	sleep 1
	"$sola/sola-river" >"$out/sola-river-2.log" 2>&1 &
	pids+=($!)
	sleep 2
	"$s" compositor windows >"$out/windows-end.txt" 2>&1
	"$s" compositor screenshot -o "$out/shot-end.png" >/dev/null 2>&1

	for p in "${pids[@]}"; do kill -9 "$p" 2>/dev/null; done
	pids=()
	sleep 0.5
	kill -TERM $rpid 2>/dev/null
	wait $rpid
	echo "river exit: $?" | tee -a "$out/steps.txt"
	ls "$out" | grep -E '^(asan|ubsan)' | sed 's/^/sanitizer report: /'
}

compare_runs() {
	local a=$1 b=$2 f
	own() {
		grep -E "^(error|warning|info|debug)(\((output|input|lock|wm|xdg|xwayland)\))?: " "$1" |
			grep -vE 'initializing (river|rivulet) version' | sed -E 's/rivulet/river/g; s/[0-9]+/N/g'
	}
	echo "== globals"
	diff "$a/globals.txt" "$b/globals.txt" && echo same
	for f in steps.txt windows-1.txt windows-end.txt randr-1.txt randr-2.txt; do
		echo "== $f"
		diff <(sed -E 's/"pid": [0-9]+/"pid": N/; s|^typed: .*/typed$|typed: ok|' "$a/$f") \
			<(sed -E 's/"pid": [0-9]+/"pid": N/; s|^typed: .*/typed$|typed: ok|' "$b/$f") && echo same
	done
	# How many manage and render sequences run depends on timing; every
	# other line River logs should be the same, as a multiset.
	cycles='(manage|render) sequence (start|finish)|finished committing transaction|sent N tracked configure'
	echo "== river's own log lines, sequence lines aside"
	diff <(own "$a/river.log" | grep -vE "$cycles" | sort) <(own "$b/river.log" | grep -vE "$cycles" | sort) && echo same
	for f in shot-typed shot-fs shot-end; do
		echo "== $f.png differing pixels: $(command compare -metric AE "$a/$f.png" "$b/$f.png" null: 2>&1)"
	done
}

case ${1:-} in
run) run "$2" "$3" ;;
compare) compare_runs "$2" "$3" ;;
*) sed -n '2,12p' "$0"; exit 1 ;;
esac
