tar -xf "$SRC/$SOURCES"
cd zstd-$VERSION/lib
make -j"$JOBS" libzstd.a
mkdir -p /out/include /out/lib/pkgconfig
cp zstd.h zdict.h zstd_errors.h /out/include/
cp libzstd.a /out/lib/
cat > /out/lib/pkgconfig/libzstd.pc <<PC
prefix=$LIVE
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: zstd
Description: fast lossless compression algorithm library
Version: $VERSION
Libs: -L\${libdir} -lzstd
Cflags: -I\${includedir}
PC
