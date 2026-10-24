# bwrap alone: no SELinux, man page or shell completions.
tar -xf "$SRC/$SOURCES"
cd "bubblewrap-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dselinux=disabled -Dman=disabled -Dtests=false \
	-Dbash_completion=disabled -Dzsh_completion=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" /out/
/out/bin/bwrap --version
