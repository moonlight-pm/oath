# iputils: ping, tracepath, arping. No capabilities or IDN; ping as a
# user uses ICMP datagram sockets.
tar -xf "$SRC/$SOURCES"
cd "iputils-$VERSION"
meson setup build --prefix="$LIVE" --buildtype=release -DUSE_CAP=false -DUSE_IDN=false \
	-DUSE_GETTEXT=false -DBUILD_MANS=false -DBUILD_HTML_MANS=false -DSKIP_TESTS=true
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" /out/
