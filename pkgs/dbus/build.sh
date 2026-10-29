# libdbus and dbus-daemon, without systemd, X11 autolaunch, docs or
# tests. Python scripts the build runs are made non-executable, so meson
# runs them with its own python. The system bus (svc:dbus-system) runs as
# void (R36), on /run/dbus/system_bus_socket, and reads BlueZ's policy
# from its slot as well as share/dbus-1/system.d and /etc/dbus-1/system.d.
tar -xf "$SRC/$SOURCES"
cd "dbus-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload --localstatedir=/var \
	-Druntime_dir=/run -Ddbus_user=void -Dsystemd=disabled -Duser_session=false -Dx11_autolaunch=disabled \
	-Ddoxygen_docs=disabled -Dducktype_docs=disabled -Dxml_docs=disabled -Dqt_help=disabled \
	-Dmodular_tests=disabled -Dinstalled_tests=false -Dselinux=disabled -Dapparmor=disabled \
	-Dlibaudit=disabled -Dlaunchd=disabled -Dkqueue=disabled -Dvalgrind=disabled \
	-Depoll=enabled -Dinotify=enabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/doc /out/lib/cmake
c=/out/share/dbus-1/system.conf
bluez=/oath/store/pkg/bluez/live/share/dbus-1/system.d
sed -i "s|^  <includedir>system.d</includedir>\$|&\n  <includedir>$bluez</includedir>|" "$c"
grep -qxF "  <includedir>$bluez</includedir>" "$c"
grep -qxF '  <user>void</user>' "$c"
grep -qxF '  <listen>unix:path=/run/dbus/system_bus_socket</listen>' "$c"
