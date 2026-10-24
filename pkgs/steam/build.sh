# Valve's launcher in libexec/steam/ (bin_steam.sh finds its bootstrap
# tarball beside itself), a two-loader ldd for steam.sh's library check,
# and bin/steam, which runs the launcher in the sandbox (R29).
tar -xzf "$SRC/$SOURCES"
v=steam-launcher
grep -qx "export STEAMSCRIPT_VERSION=$VERSION" "$v/bin_steam.sh"
d=/out/libexec/steam
mkdir -p "$d/bin" /out/bin
cp -a "$v/bin_steam.sh" "$v/bootstraplinux_ubuntu12_32.tar.xz" "$v/steam.desktop" "$v/icons" \
	"$v/client-versions.json" "$v/COPYING" "$v/steam_subscriber_agreement.txt" "$d/"
# Valve's udev rules, for the record: Oath has no udev (devices.md).
cp -a "$v/subprojects/steam-devices" "$d/"
# steam.sh asks ldd which 32-bit libraries steamui.so lacks; glibc's ldd
# knows only the x86_64 loader.
sed 's|^RTLDLIST=.*|RTLDLIST="/lib/ld-linux-x86-64.so.2 /lib/ld-linux.so.2"|' /bin/ldd >"$d/bin/ldd"
grep -qx 'RTLDLIST="/lib/ld-linux-x86-64.so.2 /lib/ld-linux.so.2"' "$d/bin/ldd"
chmod 755 "$d/bin/ldd"
cat >/out/bin/steam <<'EOF'
#!/bin/bash
# Steam's launcher in a sandbox (R29). Steam assumes a root this machine's
# does not have (Valve's distro-assumptions.md): /lib64 and /lib32, the
# i386 loader /lib/ld-linux.so.2, /usr, /sbin/ldconfig and an ld.so cache.
# bwrap gives it, and it alone, a root that has them:
#   /lib    the machine's, and ld-linux.so.2 from the packs' lib32/
#   /lib32  every pack's lib32/ (i386 libraries); /lib64 -> lib
#   /usr    bin, sbin, lib, lib32, lib64 -> those; share/, every pack's share/
#   /sbin   glibc's ldconfig and the rest of its sbin/
#   /etc    the machine's entries, ld.so.conf (/lib32) and the ld.so cache
# and the machine's other directories as they are. The tree under
# $XDG_RUNTIME_DIR/oath-steam is built once for each set of live packs.
# Steam's own containers (pressure-vessel) start inside this one and follow
# the store's links: PRESSURE_VESSEL_FILESYSTEMS_RO.
set -eu
shopt -s nullglob
live=@LIVE@
bwrap=/oath/store/pkg/bubblewrap/live/bin/bwrap
glibc=/oath/store/pkg/glibc/live
: "${XDG_RUNTIME_DIR:?steam needs XDG_RUNTIME_DIR}"
base=$XDG_RUNTIME_DIR/oath-steam
mkdir -p "$base"
key=$(for d in /oath/store/pkg/*/live; do echo "$d $(readlink "$d")"; done | sha256sum | cut -c1-16)
root=$base/$key
if [ ! -d "$root" ]; then
	t=$(mktemp -d "$base/.new.XXXXXX")
	mkdir "$t/lib" "$t/lib32" "$t/sbin" "$t/etc" "$t/usr" "$t/usr/share"
	# The machine's /lib links; its directories are bound over these.
	for e in /lib/*; do
		if [ -L "$e" ]; then cp -P "$e" "$t/lib/"; else mkdir "$t/lib/${e##*/}"; fi
	done
	# The first pack to name an entry has it, as in /lib.
	for d in /oath/store/pkg/*/live/lib32; do
		for e in "$d"/*; do
			n=$t/lib32/${e##*/}
			[ -e "$n" ] || [ -L "$n" ] || ln -s "$e" "$n"
		done
	done
	if [ -L "$t/lib32/ld-linux.so.2" ] && [ ! -e "$t/lib/ld-linux.so.2" ]; then
		ln -s "$(readlink "$t/lib32/ld-linux.so.2")" "$t/lib/ld-linux.so.2"
	fi
	for n in bin sbin lib lib32; do ln -s "/$n" "$t/usr/$n"; done
	ln -s /lib "$t/usr/lib64"
	# share/<a>/<b> from every pack: Vulkan's icd.d, fonts, X11, drirc.d.
	for s in /oath/store/pkg/*/live/share/*; do
		a=$t/usr/share/${s##*/}
		if [ -d "$s" ] && [ ! -L "$a" ]; then
			mkdir -p "$a"
			for e in "$s"/*; do
				n=$a/${e##*/}
				[ -e "$n" ] || [ -L "$n" ] || ln -s "$e" "$n"
			done
		elif [ ! -e "$a" ] && [ ! -L "$a" ]; then
			ln -s "$s" "$a"
		fi
	done
	for e in "$glibc"/sbin/*; do ln -s "$e" "$t/sbin/"; done
	echo /lib32 >"$t/etc/ld.so.conf"
	mv -T "$t" "$root" 2>/dev/null || rm -rf "$t"
fi
args=()
for e in /*; do
	case ${e##*/} in
	lib | lib32 | lib64 | usr | sbin | etc | proc | dev | sys | root) ;;
	bin | boot | oath) args+=(--ro-bind "$e" "$e") ;;
	*) args+=(--bind "$e" "$e") ;;
	esac
done
args+=(--dev-bind /dev /dev --bind /proc /proc --ro-bind /sys /sys --ro-bind "$root/lib" /lib)
for e in /lib/*; do [ -L "$e" ] || args+=(--ro-bind "$e" "$e"); done
args+=(--ro-bind "$root/lib32" /lib32 --symlink lib /lib64 --ro-bind "$root/usr" /usr
	--ro-bind "$root/sbin" /sbin --bind "$root/etc" /etc)
# The machine's /etc entries, each bound over its name (bwrap makes it).
for e in /etc/*; do
	case ${e##*/} in ld.so.conf | ld.so.cache) ;; *) args+=(--ro-bind "$e" "$e") ;; esac
done
# The 64-bit client (R29) until Valve makes it the stable one: on a first
# run, the beta channel and the client's marker. Steam's settings change
# them afterwards.
data=${XDG_DATA_HOME:-$HOME/.local/share}/Steam
if [ ! -e "$data" ]; then
	mkdir -p "$data/package"
	echo publicbeta >"$data/package/beta"
	: >"$data/.steam-enable-steamrt64-client"
fi
export DISPLAY=${DISPLAY:-:0}
export PATH=$live/libexec/steam/bin:${PATH:-/bin}
export PRESSURE_VESSEL_FILESYSTEMS_RO=/oath/store${PRESSURE_VESSEL_FILESYSTEMS_RO:+:$PRESSURE_VESSEL_FILESYSTEMS_RO}
exec "$bwrap" "${args[@]}" -- /bin/bash -c \
	'[ -e /etc/ld.so.cache ] || /sbin/ldconfig -X -i 2>/dev/null || :; exec "$@"' \
	steam "$live/libexec/steam/bin_steam.sh" "$@"
EOF
sed -i "s|@LIVE@|$LIVE|" /out/bin/steam
chmod 755 /out/bin/steam
bash -n /out/bin/steam
