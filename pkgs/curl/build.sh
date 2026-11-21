# curl and libcurl, shared (git's https), with headers, libcurl.pc and
# curl-config; mbedTLS and zlib from their packs and a pinned CA bundle.
tar -xf "$SRC/curl-$VERSION.tar.xz"
cd "curl-$VERSION"
./configure --prefix="$LIVE" --enable-shared --disable-static \
	--with-mbedtls=/oath/store/pkg/mbedtls/live --with-zlib=/oath/store/pkg/zlib/live \
	--with-ca-bundle="$LIVE/ssl/cert.pem" --without-ca-path \
	--without-libpsl --without-libidn2 --without-nghttp2 --without-brotli \
	--without-zstd --without-libssh2 --disable-ldap --disable-ldaps \
	--disable-rtsp --disable-dict --disable-telnet --disable-tftp \
	--disable-pop3 --disable-imap --disable-smtp --disable-gopher \
	--disable-mqtt --disable-manual --disable-docs
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
mkdir -p /out/ssl
cp "$SRC"/cacert-*.pem /out/ssl/cert.pem
