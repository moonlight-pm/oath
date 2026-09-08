#!/usr/bin/env bash
# Relocate glibc dbus-daemon + bluetoothd into $out as pkg:bluez.
# System bus is unix:path=/run/dbus/system_bus_socket. No udevd, no
# messagebus user, no syslog. sola-shell talks org.bluez on this bus.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_river=/oath/store/pkg/river/lib
guest_pw=/oath/store/pkg/pipewire/lib
guest_bz=/oath/store/pkg/bluez/lib
rpath="$guest_glibc:$guest_bz:$guest_pw:$guest_river"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"

DBUS=${DBUS:?}
BLUEZ=${BLUEZ:?}

if [[ -e $out ]]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec" "$out/share/dbus-1/system.d" \
	"$out/share/bluetooth" "$out/etc/bluetooth"

is_glibc() {
	case "$(basename "$1")" in
	ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
	libgcc_s.so*|libstdc++.so*)
		return 0
		;;
	*) return 1 ;;
	esac
}

# Prefer pkg:river libudev-zero over systemd libudev. Skip libsystemd.
is_skip() {
	case "$(basename "$1")" in
	libsystemd.so*|libsystemd-shared*|libudev.so*)
		return 0
		;;
	*) return 1 ;;
	esac
}

declare -A SEEN=()
declare -A SONAME=()
queue=()
loader=""

enqueue() {
	local f=$1
	[[ -e $f ]] || return 0
	local real
	real=$(readlink -f "$f")
	[[ -n ${SEEN[$real]+x} ]] && return 0
	SEEN[$real]=1
	SONAME[$real]=$(basename "$f")
	queue+=("$real")
}

dbus_bin=
for c in "$DBUS/bin/dbus-daemon" "$DBUS/bin/dbus-send" "$DBUS/bin/dbus-uuidgen"; do
	[[ -f $c ]] || { echo "relocate-bluez: missing $c" >&2; exit 1; }
done
cp -a "$DBUS/bin/dbus-daemon" "$out/libexec/dbus-daemon"
cp -a "$DBUS/bin/dbus-send" "$out/libexec/dbus-send"
cp -a "$DBUS/bin/dbus-uuidgen" "$out/libexec/dbus-uuidgen"
chmod u+w "$out/libexec/dbus-daemon" "$out/libexec/dbus-send" "$out/libexec/dbus-uuidgen"
enqueue "$out/libexec/dbus-daemon"
enqueue "$out/libexec/dbus-send"

bt=$BLUEZ/libexec/bluetooth/bluetoothd
[[ -f $bt ]] || { echo "relocate-bluez: missing $bt" >&2; exit 1; }
cp -a "$bt" "$out/libexec/bluetoothd"
chmod u+w "$out/libexec/bluetoothd"
enqueue "$out/libexec/bluetoothd"

if [[ -f $BLUEZ/bin/bluetoothctl ]]; then
	cp -a "$BLUEZ/bin/bluetoothctl" "$out/libexec/bluetoothctl"
	chmod u+w "$out/libexec/bluetoothctl"
	enqueue "$out/libexec/bluetoothctl"
fi

if [[ -d $BLUEZ/share/dbus-1/system.d ]]; then
	cp -a "$BLUEZ/share/dbus-1/system.d/." "$out/share/dbus-1/system.d/"
fi
if [[ -d $BLUEZ/etc/bluetooth ]]; then
	cp -a "$BLUEZ/etc/bluetooth/." "$out/etc/bluetooth/"
	chmod -R u+w "$out/etc/bluetooth"
fi

i=0
while [[ $i -lt ${#queue[@]} ]]; do
	f=${queue[$i]}
	i=$((i + 1))
	if [[ -z $loader ]] && file -b "$f" | grep -q ELF; then
		if loader=$(patchelf --print-interpreter "$f" 2>/dev/null); then
			enqueue "$loader"
		else
			loader=""
		fi
	fi
	if [[ -n $loader && -x $loader ]]; then
		while read -r dep; do
			[[ -e $dep ]] && enqueue "$dep"
		done < <("$loader" --list "$f" 2>/dev/null | awk '/=> \// {print $3} /^\//{print $1}')
	fi
done

for f in "${!SEEN[@]}"; do
	name=${SONAME[$f]:-$(basename "$f")}
	if is_glibc "$name" || is_skip "$name"; then
		continue
	fi
	if [[ $f == "$out"/* ]]; then
		continue
	fi
	d="$out/lib/$name"
	mkdir -p "$(dirname "$d")"
	cp -a "$f" "$d"
	chmod u+w "$d" 2>/dev/null || true
done

find "$out/libexec" "$out/lib" -type f | while read -r f; do
	file -b "$f" | grep -q ELF || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp_guest" "$f" || true
	fi
	patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
done

# Root, no fork (PID 1 supervises), no syslog, no activation helper.
cat >"$out/share/dbus-1/system.conf" <<'EOF'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>system</type>
  <user>root</user>
  <auth>EXTERNAL</auth>
  <listen>unix:path=/run/dbus/system_bus_socket</listen>
  <policy context="default">
    <allow user="*"/>
    <deny own="*"/>
    <deny send_type="method_call"/>
    <allow send_type="signal"/>
    <allow send_requested_reply="true" send_type="method_return"/>
    <allow send_requested_reply="true" send_type="error"/>
    <allow receive_type="method_call"/>
    <allow receive_type="method_return"/>
    <allow receive_type="error"/>
    <allow receive_type="signal"/>
    <allow send_destination="org.freedesktop.DBus"
           send_interface="org.freedesktop.DBus"/>
    <allow send_destination="org.freedesktop.DBus"
           send_interface="org.freedesktop.DBus.Introspectable"/>
    <allow send_destination="org.freedesktop.DBus"
           send_interface="org.freedesktop.DBus.Properties"/>
  </policy>
  <policy user="root">
    <allow own="*"/>
    <allow send_type="method_call"/>
  </policy>
  <includedir>system.d</includedir>
</busconfig>
EOF

if [[ -f $out/etc/bluetooth/main.conf ]]; then
	if ! grep -q '^Experimental' "$out/etc/bluetooth/main.conf"; then
		printf '\n[General]\nExperimental = true\n' >>"$out/etc/bluetooth/main.conf"
	fi
fi

cat >"$out/bin/dbus-daemon" <<'WRAP'
#!/bin/sh
root=/oath/store/pkg/bluez
mkdir -p /run/dbus /var/lib/dbus /etc
if [ ! -s /etc/machine-id ]; then
	if [ -x "$root/libexec/dbus-uuidgen" ]; then
		"$root/libexec/dbus-uuidgen" --ensure=/etc/machine-id 2>/dev/null || \
			"$root/libexec/dbus-uuidgen" > /etc/machine-id
	fi
fi
exec "$root/libexec/dbus-daemon" \
	--config-file="$root/share/dbus-1/system.conf" \
	--nofork --nopidfile "$@"
WRAP
cat >"$out/bin/bluetoothd" <<'WRAP'
#!/bin/sh
root=/oath/store/pkg/bluez
export DBUS_SYSTEM_BUS_ADDRESS="${DBUS_SYSTEM_BUS_ADDRESS:-unix:path=/run/dbus/system_bus_socket}"
mkdir -p /var/lib/bluetooth /var/run/bluetooth
exec "$root/libexec/bluetoothd" --nodetach "$@"
WRAP
cat >"$out/bin/bluetoothctl" <<'WRAP'
#!/bin/sh
root=/oath/store/pkg/bluez
export DBUS_SYSTEM_BUS_ADDRESS="${DBUS_SYSTEM_BUS_ADDRESS:-unix:path=/run/dbus/system_bus_socket}"
exec "$root/libexec/bluetoothctl" "$@"
WRAP
cat >"$out/bin/dbus-send" <<'WRAP'
#!/bin/sh
root=/oath/store/pkg/bluez
export DBUS_SYSTEM_BUS_ADDRESS="${DBUS_SYSTEM_BUS_ADDRESS:-unix:path=/run/dbus/system_bus_socket}"
exec "$root/libexec/dbus-send" "$@"
WRAP
chmod 755 "$out/bin/dbus-daemon" "$out/bin/bluetoothd" "$out/bin/bluetoothctl" "$out/bin/dbus-send"

cat >"$out/INDEX.md" <<'EOF'
# pkg:bluez

System D-Bus (`dbus-daemon`) plus BlueZ (`bluetoothd`) so sola-shell's
menubar Bluetooth chip can talk to `org.bluez`. Removable. No session
bus (MPRIS still out).
EOF
