# libcrypto and libssl, shared, with headers and pkg-config, and the
# openssl tool. OPENSSLDIR is the slot's ssl/ (openssl.cnf), with no CA
# certificates in it. No tests, docs or static libraries.
tar -xf "$SRC/$SOURCES"
cd "openssl-$VERSION"
perl Configure linux-x86_64 --prefix="$LIVE" --libdir=lib --openssldir="$LIVE/ssl" \
	shared no-tests no-docs
make -j"$JOBS"
make install_sw install_ssldirs DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.a
mkdir -p /out/bin /out/ssl
cp "/tmp/dest$LIVE/bin/openssl" /out/bin/
cp "/tmp/dest$LIVE"/ssl/*.cnf /out/ssl/
