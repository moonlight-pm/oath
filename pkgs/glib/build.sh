# GLib without introspection, docs, tests, libmount, SELinux or sysprof.
# Its installed Python tools (glib-mkenums, ...) name /bin/python3: the
# sandbox and Oath have no /usr/bin/env. Python scripts the build runs
# are made non-executable, so meson runs them with its own python.
tar -xf "$SRC/$SOURCES"
cd "glib-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dintrospection=disabled -Ddocumentation=false -Dman-pages=disabled -Dtests=false \
	-Dinstalled_tests=false -Dsysprof=disabled -Dselinux=disabled -Dlibmount=disabled \
	-Dlibelf=disabled -Ddtrace=disabled -Dsystemtap=disabled -Dnls=disabled \
	-Dglib_debug=disabled -Dfile_monitor_backend=inotify
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
for f in /out/bin/*; do
	sed -i '1s|^#!/usr/bin/env python3$|#!/bin/python3|' "$f"
done
rm -rf /out/share/locale /out/share/bash-completion /out/share/gdb
