# dhcpcd, the DHCP client of net:net0 (R18). Oath runs it with its own
# hook (-c /lib/oath/dhcpcd.hook), so the stock hooks are not shipped.
# Privilege separation (R36): the processes that read the network run as
# void, chrooted in its empty home. Its seccomp filter is off: a syscall
# its list lacks (a newer glibc or kernel) kills them, and a headless
# machine then loses its network.
tar -xf "$SRC/$SOURCES"
cd "dhcpcd-$VERSION"
./configure --prefix="$LIVE" --sbindir="$LIVE/bin" --libexecdir="$LIVE/libexec" \
	--sysconfdir=/etc --dbdir=/var/lib/dhcpcd --rundir=/run/dhcpcd \
	--enable-privsep --privsepuser=void --disable-seccomp --without-udev
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp "/tmp/dest$LIVE/bin/dhcpcd" /out/bin/
