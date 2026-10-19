# libxkbcommon, shared, reading xkeyboard-config's slot for keymaps and
# libX11's for Compose. No X11 or Wayland tools, no xkbregistry
# (libxml2), no docs.
tar -xf "$SRC/$SOURCES"
cd "libxkbcommon-xkbcommon-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dxkb-config-root=/oath/store/pkg/xkeyboard-config/live/share/xkeyboard-config-2 \
	-Dx-locale-root=/oath/store/pkg/libx11/live/share/X11/locale \
	-Denable-x11=false -Denable-wayland=false -Denable-xkbregistry=false -Denable-tools=false \
	-Denable-docs=false -Denable-bash-completion=false
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
