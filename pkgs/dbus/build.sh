# libdbus and dbus-daemon, without systemd, X11 autolaunch, docs or
# tests. Python scripts the build runs are made non-executable, so meson
# runs them with its own python.
tar -xf "$SRC/$SOURCES"
cd "dbus-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload --localstatedir=/var \
	-Druntime_dir=/run -Dsystemd=disabled -Duser_session=false -Dx11_autolaunch=disabled \
	-Ddoxygen_docs=disabled -Dducktype_docs=disabled -Dxml_docs=disabled -Dqt_help=disabled \
	-Dmodular_tests=disabled -Dinstalled_tests=false -Dselinux=disabled -Dapparmor=disabled \
	-Dlibaudit=disabled -Dlaunchd=disabled -Dkqueue=disabled -Dvalgrind=disabled \
	-Depoll=enabled -Dinotify=enabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/doc /out/lib/cmake
