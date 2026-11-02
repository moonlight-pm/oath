# Output: bin/conmon.
tar -xf "$SRC/$SOURCES"
cd "conmon-$VERSION"
make -j"$JOBS" bin/conmon PKG_CONFIG=pkg-config
mkdir -p /out/bin
cp bin/conmon /out/bin/
/out/bin/conmon --version | grep -q "conmon version $VERSION"
