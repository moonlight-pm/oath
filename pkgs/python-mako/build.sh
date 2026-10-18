# Pure Python, for a build's PYTHONPATH: $LIVE/lib/python.
tar -xf "$SRC/$SOURCES"
mkdir -p /out/lib/python
cp -a "mako-$VERSION/mako" /out/lib/python/
