# Pure Python, for a build's PYTHONPATH: $LIVE/lib/python. Without its C
# speedups it uses _native.
tar -xf "$SRC/$SOURCES"
mkdir -p /out/lib/python
cp -a "markupsafe-$VERSION/src/markupsafe" /out/lib/python/
rm -f /out/lib/python/markupsafe/_speedups.c /out/lib/python/markupsafe/_speedups.pyi
