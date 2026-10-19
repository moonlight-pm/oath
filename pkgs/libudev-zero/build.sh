# libudev.so.1, shared, with libudev.h and pkg-config. It enumerates
# /sys and hears hotplug on netlink group 4, where PID 1 rebroadcasts
# the kernel's uevents. patches/: upstream PR #69 (open), which marks
# sound cards SOUND_INITIALIZED and enumerates the card itself; without
# it PipeWire's ALSA monitor sees no card.
tar -xf "$SRC/$SOURCES"
cd "libudev-zero-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
make -j"$JOBS" libudev.so.1 PREFIX="$LIVE"
make install-shared PREFIX="$LIVE" DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
