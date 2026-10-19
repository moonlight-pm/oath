# libcups and its headers. The user and group are named so configure
# does not read the sandbox's /etc/passwd. patches/: 2.4.20 does not
# build without DNS-SD.
tar -xf "$SRC/$SOURCES"
cd "cups-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --sysconfdir=/etc --localstatedir=/var --with-rundir=/run/cups \
	--with-components=libcups --with-tls=no --disable-dbus --with-dnssd=no --disable-pam \
	--disable-libusb --disable-acl --with-ondemand=no \
	--with-cups-user=lp --with-cups-group=lp --with-system-groups=wheel
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.a
