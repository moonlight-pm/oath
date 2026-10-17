# libzstd, shared, with headers and pkg-config, and the zstd tool.
tar -xf "$SRC/$SOURCES"
cd "zstd-$VERSION"
make -j"$JOBS" -C lib libzstd PREFIX="$LIVE"
make -j"$JOBS" -C programs zstd PREFIX="$LIVE"
make -C lib install-shared install-includes install-pc PREFIX="$LIVE" DESTDIR=/tmp/dest
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
cp programs/zstd /out/bin/
