# Shared, with headers, pkg-config and xml2-config; no Python module, ICU or
# readline.
tar -xf "$SRC/$SOURCES"
cd "libxml2-$VERSION"
# meson.build asks git for a version suffix, and stops if there is no git.
mkdir /tmp/nogit
printf '#!/bin/sh\nexit 1\n' >/tmp/nogit/git
chmod 755 /tmp/nogit/git
PATH="/tmp/nogit:$PATH" meson setup build --prefix="$LIVE" --libdir="$LIBDIR" --buildtype=release \
	-Dpython=disabled -Dzlib=enabled -Dicu=disabled -Dreadline=disabled -Dhistory=disabled \
	-Ddocs=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man /out/share/doc /out/share/gtk-doc "/out/$LIBDIR/cmake"
[ -x /out/bin/xml2-config ]
LD_LIBRARY_PATH="/out/$LIBDIR" /out/bin/xmllint --version 2>&1 | grep -q '21504'
