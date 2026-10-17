# iproute2: ip, ss, bridge, tc. No arpd (it needs Berkeley DB).
tar -xf "$SRC/$SOURCES"
cd "iproute2-$VERSION"
sed -i /ARPD/d Makefile
m() { make PREFIX="$LIVE" SBINDIR="$LIVE/bin" NETNS_RUN_DIR=/run/netns "$@"; }
./configure
m -j"$JOBS"
m install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
[ -d "/tmp/dest$LIVE/share/iproute2" ] && mkdir -p /out/share &&
	cp -a "/tmp/dest$LIVE/share/iproute2" /out/share/
[ -d "/tmp/dest$LIVE/lib" ] && cp -a "/tmp/dest$LIVE/lib" /out/
true
