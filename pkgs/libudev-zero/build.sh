# libudev.so.1, shared, with libudev.h and pkg-config. It enumerates
# /sys and hears hotplug on netlink group 4, which something must
# rebroadcast the kernel's uevents to.
tar -xf "$SRC/$SOURCES"
cd "libudev-zero-$VERSION"
make -j"$JOBS" libudev.so.1 PREFIX="$LIVE"
make install-shared PREFIX="$LIVE" DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
