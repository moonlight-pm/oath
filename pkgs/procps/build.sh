# procps-ng: ps, pidof, free, top, kill, uptime, watch, sysctl.
tar -xf "$SRC/$SOURCES"
cd "procps-ng-$VERSION"
./configure --prefix="$LIVE" --disable-nls --disable-static --without-systemd \
	--without-elogind --with-ncurses
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" /out/
[ -d "/tmp/dest$LIVE/sbin" ] && cp -a "/tmp/dest$LIVE/sbin/." /out/bin/
find /out/lib -name '*.la' -delete
