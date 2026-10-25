# wl-copy and wl-paste. They use ext-data-control or wlr-data-control
# when the compositor offers one (rivulet offers both), else a popup
# that takes keyboard focus. The programs alone: no man pages or
# completions.
tar -xf "$SRC/$SOURCES"
cd "wl-clipboard-$VERSION"
meson setup build --prefix="$LIVE" --buildtype=release --wrap-mode=nodownload \
	-Dzshcompletiondir=no -Dfishcompletiondir=no
grep -qx '#define HAVE_EXT_DATA_CONTROL' build/src/config.h
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" /out/
/out/bin/wl-paste --version | grep -q "^wl-clipboard $VERSION$"
