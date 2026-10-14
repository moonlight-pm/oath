tar -xf "$SRC/curl-$VERSION.tar.xz"
cd curl-$VERSION
./configure --prefix=/ --disable-shared --enable-static \
	--with-mbedtls=/oath/store/pkg/mbedtls/live --with-zlib=/oath/store/pkg/zlib/live \
	--with-ca-bundle="$LIVE/ssl/cert.pem" --without-ca-path \
	--without-libpsl --without-libidn2 --without-nghttp2 --without-brotli \
	--without-zstd --without-libssh2 --disable-ldap --disable-ldaps \
	--disable-rtsp --disable-dict --disable-telnet --disable-tftp \
	--disable-pop3 --disable-imap --disable-smtp --disable-gopher \
	--disable-mqtt --disable-manual --disable-docs
make -j"$JOBS"
mkdir -p /out/bin /out/ssl
cp src/curl /out/bin/curl
cp "$SRC"/cacert-*.pem /out/ssl/cert.pem
