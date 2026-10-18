# Pure Python (lib/yaml, without libyaml), for a build's PYTHONPATH: $LIVE/lib/python.
tar -xf "$SRC/$SOURCES"
mkdir -p /out/lib/python
cp -a "pyyaml-$VERSION/lib/yaml" /out/lib/python/
