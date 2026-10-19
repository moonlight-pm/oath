# libevent, shared, with headers and pkg-config; no OpenSSL, samples or
# event_rpcgen.py.
tar -xf "$SRC/$SOURCES"
cd "libevent-$VERSION-stable"
./configure --prefix="$LIVE" --disable-static --disable-openssl --disable-samples \
	--disable-libevent-regress --disable-doxygen-doc
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
