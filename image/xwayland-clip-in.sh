#!/bin/sh
# stdin is text from `wl-paste --watch`. Own X11 CLIPBOARD so Ctrl+V
# in the rootful nest (Steam) pastes. Skip empty payloads so an image-only
# offer cannot wipe a previous text clip.
xw=/oath/store/pkg/xwayland
export PATH="$xw/libexec:/bin"
export LD_LIBRARY_PATH="$xw/lib:/oath/store/pkg/river/lib:/oath/store/pkg/glibc/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export DISPLAY="${DISPLAY:-:2}"
tmp=$(mktemp /tmp/xwayland-clip.XXXXXX) || exit 0
cat >"$tmp"
if [ -s "$tmp" ]; then
	xclip -selection clipboard -in <"$tmp"
fi
rm -f "$tmp"
