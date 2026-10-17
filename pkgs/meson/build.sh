# Meson runs from its own tree under this slot, by the python on PATH.
tar -xf "$SRC/$SOURCES"
mkdir -p /out/bin /out/lib/meson
cp -a "meson-$VERSION/meson.py" "meson-$VERSION/mesonbuild" /out/lib/meson/
cat >/out/bin/meson <<SH
#!/bin/sh
exec python3 $LIVE/lib/meson/meson.py "\$@"
SH
chmod 755 /out/bin/meson
