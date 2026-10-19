# Pango with fontconfig, FreeType and cairo; no introspection, docs,
# tests, libthai or Xft. Its utilities (pango-view, pango-list) are
# always built.
tar -xf "$SRC/$SOURCES"
cd "pango-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dintrospection=disabled -Ddocumentation=false -Dman-pages=false -Dbuild-testsuite=false \
	-Dbuild-examples=false -Dfontconfig=enabled -Dfreetype=enabled -Dcairo=enabled \
	-Dlibthai=disabled -Dxft=disabled -Dsysprof=disabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
