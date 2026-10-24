# libzstd, shared, with headers and pkg-config, and (x86_64) the zstd tool.
tar -xf "$SRC/$SOURCES"
cd "zstd-$VERSION"
make -j"$JOBS" -C lib libzstd PREFIX="$LIVE" LIBDIR="$LIVE/$LIBDIR"
make -C lib install-shared install-includes install-pc PREFIX="$LIVE" LIBDIR="$LIVE/$LIBDIR" \
	DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
# A pack's bin/ is linked into /bin: the tool is the x86_64 pack's.
if [ "$LIBDIR" = lib ]; then
	make -j"$JOBS" -C programs zstd PREFIX="$LIVE"
	mkdir -p /out/bin
	cp programs/zstd /out/bin/
fi
