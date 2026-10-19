# FreeType, shared, with zlib and without HarfBuzz (which builds on it),
# PNG, bzip2 or Brotli. The sandbox has no /usr/bin/env: Python scripts
# that are not executable run through meson's python.
tar -xf "$SRC/$SOURCES"
cd "freetype-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Ddefault_library=shared -Dbrotli=disabled -Dbzip2=disabled -Dharfbuzz=disabled \
	-Dpng=disabled -Dzlib=system -Dhvf=disabled -Dtests=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
