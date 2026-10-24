# The Wayland libraries and wayland-scanner, without docs, tests or DTD
# validation (libxml2). For i386 the libraries alone: the build runs the
# x86_64 pack's scanner, and a pack's bin/ is linked into /bin.
tar -xf "$SRC/$SOURCES"
cd "wayland-$VERSION"
scanner=true
[ "$LIBDIR" = lib ] || scanner=false
meson setup build --prefix="$LIVE" --libdir="$LIBDIR" --buildtype=release \
	-Ddocumentation=false -Ddtd_validation=false -Dtests=false -Dscanner=$scanner
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
for d in bin share; do
	if [ -d "/tmp/dest$LIVE/$d" ]; then cp -a "/tmp/dest$LIVE/$d" /out/; fi
done
