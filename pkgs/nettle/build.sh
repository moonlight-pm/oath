# libnettle, shared, with headers and pkg-config. No hogweed (public-key
# code, GMP), no tools.
tar -xf "$SRC/$SOURCES"
cd "nettle-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/lib" --disable-static --disable-documentation \
	--disable-openssl --disable-public-key
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
