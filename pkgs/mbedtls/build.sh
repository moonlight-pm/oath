tar -xf "$SRC/$SOURCES"
cd mbedtls-$VERSION
make -j"$JOBS" lib
mkdir -p /out/include /out/lib
cp -a include/mbedtls include/psa /out/include/
cp library/libmbedcrypto.a library/libmbedx509.a library/libmbedtls.a /out/lib/
