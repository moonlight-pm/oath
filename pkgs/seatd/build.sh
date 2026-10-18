# seatd, seatd-launch and libseat with the seatd and builtin backends
# (noop is always there: LIBSEAT_BACKEND=noop opens devices directly).
# No logind.
tar -xf "$SRC/$SOURCES"
cd "seatd-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dlibseat-logind=disabled -Dlibseat-seatd=enabled -Dlibseat-builtin=enabled \
	-Dserver=enabled -Dexamples=disabled -Dman-pages=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
