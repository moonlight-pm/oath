#!/bin/bash
# Prism Launcher in a box (R74). A mod is Java code from the internet and
# runs with the rights of whoever plays, and `home` has sudo with no password,
# so the launcher and every game it starts see only:
#   /            the machine's root, read-only (the store, /lib, /etc)
#   Prism's folder  ${XDG_DATA_HOME:-~/.local/share}/PrismLauncher, read-write:
#                instances, worlds, mods, screenshots, accounts
#   a cache      ${XDG_CACHE_HOME:-~/.cache}/PrismLauncher, as ~/.cache
#                (shader caches, fontconfig)
#   the display  the Wayland socket, and Xwayland's for the game (X11)
#   sound        PipeWire's and pipewire-pulse's sockets
#   the GPU      /dev/dri and NVIDIA's nodes
#   pads         /dev/input (only pads are the seat's there, R44)
#   the network  the machine's
# and not the rest of /home, /run (PID 1's socket, D-Bus, the desk's other
# sockets) or /dev (not /dev/uinput, which can type into the desk). bwrap
# sets no_new_privs, so sudo cannot run in it. Java is every pkg:jre-* on
# the machine (PRISMLAUNCHER_JAVA_PATHS).
set -eu
shopt -s nullglob
live=@LIVE@
bwrap=/oath/store/pkg/bubblewrap/live/bin/bwrap
qt=/oath/store/pkg
run=${XDG_RUNTIME_DIR:?prismlauncher needs XDG_RUNTIME_DIR}
data=${XDG_DATA_HOME:-$HOME/.local/share}/PrismLauncher
cache=${XDG_CACHE_HOME:-$HOME/.cache}/PrismLauncher
mkdir -p "$data" "$cache"

wayland=${WAYLAND_DISPLAY:-}
if [ -z "$wayland" ]; then
	for s in "$run"/wayland-*; do [ -S "$s" ] && wayland=${s##*/} && break; done
fi
[ -n "$wayland" ] || { echo "prismlauncher: no Wayland display" >&2; exit 1; }
case $wayland in /*) socket=$wayland ;; *) socket=$run/$wayland ;; esac

javas=
for j in /oath/store/pkg/jre-*/live/libexec/jre-*/bin/java; do javas=${javas:+$javas:}$j; done

args=(
	--ro-bind / /
	--dev /dev
	--proc /proc
	--tmpfs /tmp
	--tmpfs /run
	--tmpfs /home
	--perms 0700 --dir "$run"
	--bind "$data" "$data"
	--bind "$cache" "$HOME/.cache"
	--ro-bind "$socket" "$socket"
	--unshare-pid --unshare-uts --unshare-cgroup-try
	--die-with-parent --new-session
	--unsetenv DBUS_SESSION_BUS_ADDRESS --unsetenv DBUS_SYSTEM_BUS_ADDRESS
	--unsetenv SSH_AUTH_SOCK --unsetenv XDG_CACHE_HOME
	--setenv WAYLAND_DISPLAY "$wayland"
	--setenv QT_QPA_PLATFORM wayland
	--setenv QT_PLUGIN_PATH "$qt/qtsvg/live/plugins:$qt/qtimageformats/live/plugins"
	--setenv PRISMLAUNCHER_JAVA_PATHS "$javas"
)
# Xwayland: its socket by path; the abstract one is reachable anyway, since
# the box keeps the machine's network.
if [ -n "${DISPLAY:-}" ] && [ -S "/tmp/.X11-unix/X${DISPLAY#*:}" ]; then
	args+=(--ro-bind "/tmp/.X11-unix/X${DISPLAY#*:}" "/tmp/.X11-unix/X${DISPLAY#*:}")
	[ -n "${XAUTHORITY:-}" ] && [ -f "$XAUTHORITY" ] && args+=(--ro-bind "$XAUTHORITY" "$XAUTHORITY")
fi
for s in "$run/pipewire-0" "$run/pulse/native"; do
	[ -S "$s" ] && args+=(--ro-bind "$s" "$s")
done
for d in /dev/dri /dev/nvidia[0-9]* /dev/nvidiactl /dev/nvidia-modeset /dev/nvidia-uvm \
	/dev/nvidia-uvm-tools /dev/nvidia-caps /dev/input; do
	[ -e "$d" ] && args+=(--dev-bind "$d" "$d")
done

exec "$bwrap" "${args[@]}" -- "$live/libexec/prismlauncher/bin/prismlauncher" "$@"
