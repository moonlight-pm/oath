# dhcpcd, the DHCP client of net:net0 (R18). Oath runs it with its own
# hook (-c /lib/oath/dhcpcd.hook), so the stock hooks are not shipped.
tar -xf "$SRC/$SOURCES"
cd "dhcpcd-$VERSION"
./configure --prefix="$LIVE" --sbindir="$LIVE/bin" --libexecdir="$LIVE/libexec" \
	--sysconfdir=/etc --dbdir=/var/lib/dhcpcd --rundir=/run/dhcpcd \
	--disable-privsep --without-udev
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp "/tmp/dest$LIVE/bin/dhcpcd" /out/bin/
