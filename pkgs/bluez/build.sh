# BlueZ for svc:bluetooth: bluetoothd in libexec/bluetooth/, its D-Bus
# policy in share/dbus-1/system.d/ (pkg:dbus's system bus reads it from
# this slot), its state in /var/lib/bluetooth and /etc/bluetooth its
# configuration directory (none ships: the defaults serve).
tar -xf "$SRC/$SOURCES"
cd "bluez-$VERSION"
./configure --prefix="$LIVE" --libexecdir="$LIVE/libexec" --sysconfdir=/etc --localstatedir=/var \
	--with-dbusconfdir="$LIVE/share" --with-dbussystembusdir="$LIVE/share/dbus-1/system-services" \
	--with-dbussessionbusdir="$LIVE/share/dbus-1/services" \
	--enable-library --disable-systemd --disable-udev --disable-cups --disable-obex \
	--disable-mesh --disable-midi --disable-manpages --disable-test \
	--disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/libexec" \
	"/tmp/dest$LIVE/share" /out/
rm -rf /out/share/man /out/share/doc /out/share/zsh
[ -x /out/libexec/bluetooth/bluetoothd ]
[ -f /out/share/dbus-1/system.d/bluetooth.conf ]
