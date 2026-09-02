# Sourced by /bin/river and sola-* wrappers.
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
else
	unset WLR_NO_HARDWARE_CURSORS
	unset LIBGL_ALWAYS_SOFTWARE
fi
