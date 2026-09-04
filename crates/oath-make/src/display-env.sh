# Sourced by /bin/river and sola-* wrappers.
# Display TZ from host:local.timezone (T34). Empty → leave unset (UTC).
if [ -f /oath/objects/host/local/desired.json ]; then
	_oath_tz=$(sed -n 's/.*"timezone"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
		/oath/objects/host/local/desired.json | head -n 1)
	if [ -n "$_oath_tz" ]; then
		export TZ="$_oath_tz"
	fi
	unset _oath_tz
fi
# virtio-gpu (QEMU) often has no cursor plane and no virgl — software
# cursor + llvmpipe. Real KMS (amdgpu, i915, …) must not force that:
# hardware cursors are what sola-scope needs (sprite on its own plane).
oath_virtio_kms=0
for _oath_c in /sys/class/drm/card[0-9]; do
	_oath_d=$(readlink -f "$_oath_c/device/driver" 2>/dev/null) || continue
	case "$_oath_d" in
	*virtio*)
		oath_virtio_kms=1
		break
		;;
	esac
done
unset _oath_c _oath_d
if [ "$oath_virtio_kms" = 1 ]; then
	export WLR_NO_HARDWARE_CURSORS=1
	export LIBGL_ALWAYS_SOFTWARE=1
	export WLR_RENDERER=pixman
else
	unset WLR_NO_HARDWARE_CURSORS
	unset LIBGL_ALWAYS_SOFTWARE
	# pixman compositor has no linux-dmabuf. iced/wgpu then falls through
	# zink + llvmpipe and typing in the launcher lags. gles2 on amdgpu
	# (radeonsi) is the hardware path. virtio stays pixman (no virgl).
	export WLR_RENDERER=gles2
fi
# Grok (webbrowser crate) and other guests exec xdg-open / read mimeapps.
export BROWSER="${BROWSER:-/bin/xdg-open}"
export XDG_DATA_DIRS="${XDG_DATA_DIRS:-/oath/store/pkg/sola/share}"
export XDG_CONFIG_DIRS="${XDG_CONFIG_DIRS:-/oath/store/pkg/sola/etc/xdg}"
