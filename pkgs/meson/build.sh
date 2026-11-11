# Meson runs from its own tree under this slot, by the python on PATH.
# The tree holds no bytecode, and a build must not write any into it: where
# the build sandbox could not make its packs read-only (the audit's box,
# 2026-11-10), python3 put __pycache__ beside mesonbuild/, and every later
# build refused the changed tree until this one replaced it.
tar -xf "$SRC/$SOURCES"
mkdir -p /out/bin /out/lib/meson
cp -a "meson-$VERSION/meson.py" "meson-$VERSION/mesonbuild" /out/lib/meson/
cat >/out/bin/meson <<SH
#!/bin/sh
exec python3 $LIVE/lib/meson/meson.py "\$@"
SH
chmod 755 /out/bin/meson
