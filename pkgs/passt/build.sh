# Output: bin/passt, bin/pasta (a link) and their AVX2 builds, bin/qrap.
tar -xf "$SRC/$SOURCES"
cd "passt-$VERSION"
make -j"$JOBS" VERSION="$VERSION" prefix="$LIVE"
make install DESTDIR=/tmp/dest VERSION="$VERSION" prefix="$LIVE"
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/bin/." /out/bin/
/out/bin/pasta --version | grep -q "$VERSION"
