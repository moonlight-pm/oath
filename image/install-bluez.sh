#!/bin/bash
# Pack pkg:bluez on the build host (nix dbus + bluez) and live-install
# on this Oath box, or rsync to canto. System dbus + bluetoothd.
set -euo pipefail

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
stage=${OATH_STAGE:-$root/build/bluez-pack}

as_root() {
	if [ "$(id -u)" = 0 ]; then "$@"; else sudo -n "$@"; fi
}

if [ ! -f /oath/INDEX.md ]; then
	# Build host: relocate, then copy to canto if ssh works.
	dbus=${DBUS:-${OATH_DBUS:-}}
	bluez=${BLUEZ:-${OATH_BLUEZ:-}}
	if [ -z "$dbus" ] || [ ! -d "$dbus" ]; then
		dbus=$(nix-build --no-out-link '<nixpkgs>' -A dbus)
	fi
	if [ -z "$bluez" ] || [ ! -d "$bluez" ]; then
		bluez=$(nix-build --no-out-link '<nixpkgs>' -A bluez)
	fi
	echo "==> relocate bluez  dbus=$dbus  bluez=$bluez"
	chmod +x "$here/relocate-bluez.sh"
	DBUS=$dbus BLUEZ=$bluez bash "$here/relocate-bluez.sh" "$stage"
	if [ -d /oath/store/pkg ]; then
		:
	elif command -v ssh >/dev/null 2>&1; then
		echo "==> copy to canto"
		tar -C "$stage" -cf - . | ssh -o BatchMode=yes home@canto \
			'sudo -n rm -rf /tmp/bluez-pack && sudo -n mkdir -p /tmp/bluez-pack && sudo -n tar -C /tmp/bluez-pack -xf -'
		ssh -o BatchMode=yes home@canto 'sudo -n env OATH_STAGE=/tmp/bluez-pack sh -s' <"$here/install-bluez.sh"
		exit 0
	else
		echo "relocated $stage (copy onto the appliance)"
		exit 0
	fi
fi

stage=${OATH_STAGE:-$stage}
[ -x "$stage/libexec/dbus-daemon" ] || { echo "install-bluez: no $stage/libexec/dbus-daemon" >&2; exit 1; }

echo "==> install pkg:bluez"
as_root rm -rf /oath/store/pkg/bluez
as_root mkdir -p /oath/store/pkg
as_root cp -a "$stage" /oath/store/pkg/bluez
as_root chmod -R u+rX /oath/store/pkg/bluez
for b in dbus-daemon bluetoothd bluetoothctl dbus-send; do
	as_root ln -sfn /oath/store/pkg/bluez/bin/$b /bin/$b
done

write_svc() {
	name=$1
	exec_json=$2
	wants_json=$3
	dir=/oath/objects/svc/$name
	as_root mkdir -p "$dir"
	as_root tee "$dir/desired.json" >/dev/null <<JSON
{
  "enabled": true,
  "exec": $exec_json,
  "restart": "always",
  "wants": $wants_json
}
JSON
	as_root tee "$dir/actual.json" >/dev/null <<JSON
{ "state": "stopped", "pid": null, "restarts": 0 }
JSON
	as_root tee "$dir/meta.json" >/dev/null <<JSON
{ "id": "svc:$name", "kind": "svc", "name": "$name", "safety": "mutate", "status": "drift" }
JSON
}

write_svc dbus '["/bin/dbus-daemon"]' '[]'
write_svc bluetoothd '["/bin/bluetoothd"]' '["svc:dbus"]'

as_root mkdir -p /oath/objects/pkg/bluez
as_root tee /oath/objects/pkg/bluez/desired.json >/dev/null <<'JSON'
{ "present": true }
JSON
as_root tee /oath/objects/pkg/bluez/actual.json >/dev/null <<'JSON'
{ "present": true, "links": ["bluetoothctl","bluetoothd","dbus-daemon","dbus-send"], "removable": true }
JSON
as_root tee /oath/objects/pkg/bluez/meta.json >/dev/null <<'JSON'
{ "id": "pkg:bluez", "kind": "pkg", "name": "bluez", "safety": "mutate", "status": "in-sync" }
JSON

if [ "$(id -u)" = 0 ]; then
	oath apply svc:dbus svc:bluetoothd
else
	sudo -n oath apply svc:dbus svc:bluetoothd
fi
sleep 1
echo "==> courage"
ls -l /run/dbus/system_bus_socket /bin/dbus-daemon /bin/bluetoothd
ps -o user,pid,args | grep -E 'dbus-daemon|bluetoothd' | grep -v grep || true
echo done
