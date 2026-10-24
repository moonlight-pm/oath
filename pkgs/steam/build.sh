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
# Valve's user-namespace probe (steam-runtime-tools' bwrap.c) runs `true`
# in a root of /usr, /bin, /lib*, /sbin and /etc alone, where every link
# into /oath/store dangles. This true is a static ELF of 129 bytes (exit
# 0); the sandbox puts it at /usr/local/bin/true, which the probe sees.
printf '\177ELF\2\1\1\0\0\0\0\0\0\0\0\0\2\0\076\0\1\0\0\0\170\0\100\0\0\0\0\0\100\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\100\0\070\0\1\0\0\0\0\0\0\0\1\0\0\0\5\0\0\0\0\0\0\0\0\0\0\0\0\0\100\0\0\0\0\0\0\0\100\0\0\0\0\0\201\0\0\0\0\0\0\0\201\0\0\0\0\0\0\0\0\020\0\0\0\0\0\0\061\377\270\074\0\0\0\017\005' >"$d/true"
chmod 755 "$d/true"
"$d/true"
[ "$(sha256sum <"$d/true")" = "bab0ab5ef9f39630434e962db2c56ba8be4e1f8a4b3203de5cdc89e1ebd53791  -" ]
cat >/out/bin/steam <<'EOF'
#!/bin/bash
# Steam's launcher in a sandbox (R29). Steam assumes a root this machine's
# does not have (Valve's distro-assumptions.md): /usr, /lib64 and /lib32,
# the i386 loader /lib/ld-linux.so.2, /sbin/ldconfig and an ld.so cache.
# bwrap gives it, and it alone, a merged-/usr root that has them:
#   /usr/bin, /usr/sbin, /usr/lib  the machine's /bin, /sbin and /lib links,
#           with glibc's sbin/ and the i386 loader from the packs' lib32/
#   /usr/lib32  every pack's lib32/ (i386 libraries)
#   /usr/share  every pack's share/
#   /usr/local/bin/true  static, for Valve's user-namespace probe
#   /bin, /sbin, /lib, /lib32, /lib64  -> usr/bin, usr/sbin, usr/lib, …
#   /etc    the machine's entries, ld.so.conf (/lib32), the ld.so cache and
#           the CA bundle (pkg:curl's) at the paths other distributions use
# and the machine's other directories as they are. The links are real
# directories under /usr because Steam's containers mount it elsewhere
# (/run/host/usr). The tree under $XDG_RUNTIME_DIR/oath-steam is built
# once for each set of live packs. Steam's containers (pressure-vessel)
# start inside this one and follow the store's links:
# PRESSURE_VESSEL_FILESYSTEMS_RO.
set -eu
shopt -s nullglob
live=@LIVE@
bwrap=/oath/store/pkg/bubblewrap/live/bin/bwrap
glibc=/oath/store/pkg/glibc/live
: "${XDG_RUNTIME_DIR:?steam needs XDG_RUNTIME_DIR}"
base=$XDG_RUNTIME_DIR/oath-steam
mkdir -p "$base"
key=$({ cat "$0"; for d in /oath/store/pkg/*/live; do echo "$d $(readlink "$d")"; done; } |
	sha256sum | cut -c1-16)
root=$base/$key
# The first to name an entry of dir $1 has it, as in the machine's farms.
link() { [ -e "$1/${2##*/}" ] || [ -L "$1/${2##*/}" ] || ln -s "$2" "$1/"; }
if [ ! -d "$root" ]; then
	t=$(mktemp -d "$base/.new.XXXXXX")
	u=$t/usr
	mkdir "$t/etc" "$u" "$u/bin" "$u/sbin" "$u/lib" "$u/lib32" "$u/share" "$u/local" "$u/local/bin"
	# The machine's links; /lib's directories are bound over these.
	for d in bin sbin lib; do
		for e in "/$d"/*; do
			if [ -L "$e" ]; then cp -P "$e" "$u/$d/"; else mkdir "$u/$d/${e##*/}"; fi
		done
	done
	for e in "$glibc"/sbin/*; do link "$u/sbin" "$e"; done
	for d in /oath/store/pkg/*/live/lib32; do
		for e in "$d"/*; do link "$u/lib32" "$e"; done
	done
	[ -L "$u/lib32/ld-linux.so.2" ] && link "$u/lib" "$(readlink "$u/lib32/ld-linux.so.2")"
	ln -s lib "$u/lib64"
	cp "$live/libexec/steam/true" "$u/local/bin/true"
	# share/<a>/<b> from every pack: Vulkan's icd.d, fonts, X11, drirc.d.
	for s in /oath/store/pkg/*/live/share/*; do
		a=$u/share/${s##*/}
		if [ -d "$s" ] && [ ! -L "$a" ]; then
			mkdir -p "$a"
			for e in "$s"/*; do link "$a" "$e"; done
		else
			link "$u/share" "$s"
		fi
	done
	echo /lib32 >"$t/etc/ld.so.conf"
	# The machine's CA bundle where Steam's own curl looks for one.
	mkdir -p "$t/etc/ssl/certs" "$t/etc/pki/tls/certs"
	for n in ssl/certs/ca-certificates.crt ssl/cert.pem pki/tls/certs/ca-bundle.crt; do
		ln -s /oath/store/pkg/curl/live/ssl/cert.pem "$t/etc/$n"
	done
	mv -T "$t" "$root" 2>/dev/null || rm -rf "$t"
fi
args=()
for e in /*; do
	case ${e##*/} in
	bin | sbin | lib | lib32 | lib64 | usr | etc | proc | dev | sys | root) ;;
	boot | oath) args+=(--ro-bind "$e" "$e") ;;
	*) args+=(--bind "$e" "$e") ;;
	esac
done
args+=(--ro-bind "$root/usr" /usr --dev-bind /dev /dev --bind /proc /proc --ro-bind /sys /sys)
for n in bin sbin lib lib32; do args+=(--symlink "usr/$n" "/$n"); done
args+=(--symlink usr/lib /lib64)
for e in /lib/*; do [ -L "$e" ] || args+=(--ro-bind "$e" "/usr/lib/${e##*/}"); done
# The machine's /etc entries, each bound over its name (bwrap makes it).
args+=(--bind "$root/etc" /etc)
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
export PATH=$live/libexec/steam/bin:${PATH:-/bin}:/usr/local/bin
export PRESSURE_VESSEL_FILESYSTEMS_RO=/oath/store${PRESSURE_VESSEL_FILESYSTEMS_RO:+:$PRESSURE_VESSEL_FILESYSTEMS_RO}
exec "$bwrap" "${args[@]}" -- /bin/bash -c \
	'[ -e /etc/ld.so.cache ] || /sbin/ldconfig -X -i 2>/dev/null || :; exec "$@"' \
	steam "$live/libexec/steam/bin_steam.sh" "$@"
EOF
sed -i "s|@LIVE@|$LIVE|" /out/bin/steam
chmod 755 /out/bin/steam
bash -n /out/bin/steam
