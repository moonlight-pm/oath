# Shared, with headers and pkg-config; no programs, tests or static library.
tar -xf "$SRC/$SOURCES"
cd "libarchive-$VERSION"
# bzip2, a base pack, has no pkg-config file, so its prefix is named.
prefixes=$(printf %s "$PKG_CONFIG_PATH" | tr : '\n' | sed 's|/lib/pkgconfig$||' | paste -sd ';')
prefixes="$prefixes;/oath/store/pkg/bzip2/live"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DCMAKE_PREFIX_PATH="$prefixes" \
	-DENABLE_OPENSSL=OFF -DENABLE_MBEDTLS=OFF -DENABLE_NETTLE=OFF -DENABLE_LIBB2=OFF \
	-DENABLE_LZ4=OFF -DENABLE_LZO=OFF -DENABLE_LIBXML2=OFF -DENABLE_BSDXML=OFF -DENABLE_EXPAT=OFF \
	-DENABLE_PCREPOSIX=OFF -DENABLE_PCRE2POSIX=OFF -DENABLE_XATTR=OFF -DENABLE_ACL=OFF \
	-DENABLE_TAR=OFF -DENABLE_CPIO=OFF -DENABLE_CAT=OFF -DENABLE_UNZIP=OFF -DENABLE_TEST=OFF \
	-DENABLE_ZLIB=ON -DENABLE_BZip2=ON -DENABLE_LZMA=ON -DENABLE_ZSTD=ON
for h in ZLIB_H BZLIB_H LZMA_H ZSTD_H; do grep -qx "#define HAVE_$h 1" build/config.h; done
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man "/out/$LIBDIR"/*.a
