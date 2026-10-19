# dig, host and nslookup, and the BIND libraries they load. named and
# the other tools are not built; no DNS over HTTPS (nghttp2), IDN,
# readline, GeoIP or jemalloc.
tar -xf "$SRC/$SOURCES"
cd "bind-$VERSION"
./configure --prefix="$LIVE" --sysconfdir=/etc --localstatedir=/var --disable-static \
	--disable-doh --disable-geoip --disable-dnstap --without-jemalloc \
	--without-libxml2 --without-json-c --without-lmdb --without-libidn2 \
	--without-readline --without-zlib --without-gssapi --without-cmocka
make -j"$JOBS" -C lib
make -j"$JOBS" -C bin/dig
make -C lib install DESTDIR=/tmp/dest
make -C bin/dig install DESTDIR=/tmp/dest
mkdir -p /out/lib
cp -a "/tmp/dest$LIVE/bin" /out/
for l in isc dns ns isccfg; do
	cp -a "/tmp/dest$LIVE"/lib/lib$l-*.so /out/lib/
done
