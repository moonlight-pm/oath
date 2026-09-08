#!/bin/sh
# River session Xwayland. SI glamor SIGBUS'd on tiled BOs; -glamor off
# unless the caller already passed -glamor (gamescope nested X).
real=/oath/store/pkg/xwayland/libexec/Xwayland
if [ ! -x "$real" ]; then
	real=/oath/store/pkg/xwayland/bin/Xwayland.real
fi
if [ ! -x "$real" ]; then
	echo "xwayland-oath: no Xwayland ELF" >&2
	exit 1
fi
case " $* " in
*" -glamor "*) exec "$real" "$@" ;;
*) exec "$real" "$@" -glamor off ;;
esac
