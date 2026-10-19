# cairo's image surfaces with FreeType and fontconfig; no X11, PNG, PDF,
# PostScript or GLib. Python scripts the build runs are made
# non-executable, so meson runs them with its own python.
tar -xf "$SRC/$SOURCES"
cd "cairo-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dfreetype=enabled -Dfontconfig=enabled -Dpng=disabled -Dzlib=disabled -Dxlib=disabled \
	-Dxcb=disabled -Dxlib-xcb=disabled -Dquartz=disabled -Ddwrite=disabled -Dtee=disabled \
	-Dtests=disabled -Dlzo=disabled -Dgtk2-utils=disabled -Dglib=disabled -Dspectre=disabled \
	-Dsymbol-lookup=disabled -Dgtk_doc=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
