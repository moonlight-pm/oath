# libharfbuzz with FreeType and GLib; no ICU, cairo, subset, raster or
# utilities. Python scripts the build runs are made non-executable, so
# meson runs them with its own python.
tar -xf "$SRC/$SOURCES"
cd "harfbuzz-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dfreetype=enabled -Dglib=enabled -Dgobject=disabled -Dcairo=disabled -Dchafa=disabled \
	-Dpng=disabled -Dzlib=disabled -Dicu=disabled -Dgraphite2=disabled -Dfontations=disabled \
	-Dharfrust=disabled -Dkbts=disabled -Dwasm=disabled -Draster=disabled -Dvector=disabled \
	-Dgpu=disabled -Dgpu_demo=disabled -Dsubset=disabled -Dtests=disabled \
	-Dintrospection=disabled -Ddocs=disabled -Dutilities=disabled -Dbenchmark=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -rf /out/lib/cmake
