# libinput, shared, without libwacom, mtdev (old protocol-A touch
# devices) or Lua plugins; devices come from libudev-zero. The libinput
# tool keeps its C commands (list-devices, debug-events); its Python
# ones, the test runner and udev's rules and callouts are dropped.
tar -xf "$SRC/$SOURCES"
cd "libinput-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dlibwacom=false -Dmtdev=false -Ddebug-gui=false -Dtests=false -Dinstall-tests=false \
	-Ddocumentation=false -Dlua-plugins=disabled -Dautoload-plugins=false -Dzshcompletiondir=no
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cd "/tmp/dest$LIVE"
for f in libexec/libinput/*; do
	case $(head -c 2 "$f") in '#!') rm "$f" ;; esac
done
rm -f libexec/libinput/libinput-test*
cp -a bin include lib libexec /out/
rm -rf /out/lib/udev
mkdir -p /out/share
cp -a share/libinput /out/share/
