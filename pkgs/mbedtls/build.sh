# Mbed TLS, shared (curl's TLS).
tar -xf "$SRC/$SOURCES"
cd "mbedtls-$VERSION"
make -j"$JOBS" SHARED=1 lib
mkdir -p /out/include /out/lib
cp -a include/mbedtls include/psa /out/include/
cp -a library/libmbed*.so* /out/lib/
